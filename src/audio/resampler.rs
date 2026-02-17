use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction, Resampler};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

use crate::config::ResamplingQuality;
use crate::sse::SseEvent;
use super::engine::SlotId;
use super::LoadedTrack;

pub struct ResamplingJob {
    pub slot: SlotId,
    pub id: String,
    pub track: Arc<LoadedTrack>,
    pub cancellation: Option<Arc<AtomicBool>>,
}

pub fn spawn_resampling(
    jobs: Vec<ResamplingJob>,
    target_rate: u32,
    quality: ResamplingQuality,
    broadcast_tx: Option<Arc<broadcast::Sender<SseEvent>>>,
) {
    if target_rate == 0 {
        return;
    }

    for job in jobs {
        let from_rate = job.track.original_rate;
        let slot_name = job.slot.name().to_string();
        let program_id = job.id.clone();

        if from_rate == target_rate {
            if let Some(ref tx) = broadcast_tx {
                let _ = tx.send(SseEvent::ResamplingProgress {
                    slot: slot_name.clone(),
                    program_id: program_id.clone(),
                    track_name: program_id.clone(),
                    current: 0,
                    total: 0,
                    active: false,
                    from_rate: 0,
                    to_rate: 0,
                });
                let _ = tx.send(SseEvent::ResamplingComplete {
                    slot: slot_name,
                    program_id: program_id.clone(),
                    target_rate,
                    quality: quality.cache_key().to_string(),
                });
            }
            continue;
        }

        let track = job.track;
        let cancelled = job.cancellation.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let broadcast_tx_clone = broadcast_tx.clone();

        tokio::spawn(async move {
            let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<(u32, u32)>(100);

            let slot_for_forwarder = slot_name.clone();
            let program_id_for_forwarder = program_id.clone();
            let broadcast_for_forwarder = broadcast_tx_clone.clone();

            let progress_forwarder = tokio::spawn(async move {
                if let Some(tx) = broadcast_for_forwarder {
                    while let Some((current, total)) = progress_rx.recv().await {
                        let _ = tx.send(SseEvent::ResamplingProgress {
                            slot: slot_for_forwarder.clone(),
                            program_id: program_id_for_forwarder.clone(),
                            track_name: program_id_for_forwarder.clone(),
                            current,
                            total,
                            active: true,
                            from_rate,
                            to_rate: target_rate,
                        });
                    }
                }
            });

            let cancelled_clone = Arc::clone(&cancelled);
            let id_for_log = program_id.clone();
            let track_for_save = Arc::clone(&track);
            let result = tokio::task::spawn_blocking(move || {
                let callback = move |current: u32, total: u32| -> bool {
                    let _ = progress_tx.blocking_send((current, total));
                    !cancelled_clone.load(Ordering::Relaxed)
                };
                track.resample_to_memory(target_rate, quality, Some(callback))
            })
            .await;

            let _ = progress_forwarder.await;

            if let Some(ref tx) = broadcast_tx_clone {
                let _ = tx.send(SseEvent::ResamplingProgress {
                    slot: slot_name.clone(),
                    program_id: program_id.clone(),
                    track_name: program_id.clone(),
                    current: 0,
                    total: 0,
                    active: false,
                    from_rate: 0,
                    to_rate: 0,
                });
            }

            match result {
                Ok(Ok(())) => {
                    eprintln!("[Resampler] Complete for '{}'", id_for_log);
                    if let Some(ref tx) = broadcast_tx_clone {
                        let _ = tx.send(SseEvent::ResamplingComplete {
                            slot: slot_name.clone(),
                            program_id: program_id.clone(),
                            target_rate,
                            quality: quality.cache_key().to_string(),
                        });
                    }

                    tokio::task::spawn_blocking(move || {
                        if let Some(samples) = track_for_save.get_resampled_samples(target_rate) {
                            track_for_save.save_to_cache(target_rate, quality, &samples);
                        }
                    });
                }
                Ok(Err(e)) if e.contains("cancelled") => {
                    eprintln!("[Resampler] Cancelled for '{}'", id_for_log)
                }
                Ok(Err(e)) => {
                    eprintln!("[Resampler] Failed for '{}': {}", id_for_log, e)
                }
                Err(e) => eprintln!("[Resampler] Task failed for '{}': {}", id_for_log, e),
            }
        });
    }
}

pub fn resample(
    samples: &[f32],
    channels: u16,
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, String> {
    resample_with_options(samples, channels, from_rate, to_rate, ResamplingQuality::default(), None::<fn(u32, u32) -> bool>)
}

pub fn resample_with_progress<F>(
    samples: &[f32],
    channels: u16,
    from_rate: u32,
    to_rate: u32,
    progress_callback: Option<F>,
) -> Result<Vec<f32>, String>
where
    F: Fn(u32, u32) -> bool,
{
    resample_with_options(samples, channels, from_rate, to_rate, ResamplingQuality::default(), progress_callback)
}

pub fn resample_with_options<F>(
    samples: &[f32],
    channels: u16,
    from_rate: u32,
    to_rate: u32,
    quality: ResamplingQuality,
    progress_callback: Option<F>,
) -> Result<Vec<f32>, String>
where
    F: Fn(u32, u32) -> bool,
{
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }

    let channels = channels as usize;
    if channels == 0 {
        return Err("Invalid channel count: 0".to_string());
    }

    let frames_in = samples.len() / channels;
    if frames_in == 0 {
        return Ok(Vec::new());
    }

    let params = SincInterpolationParameters {
        sinc_len: quality.sinc_len(),
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Cubic,
        oversampling_factor: quality.oversampling_factor(),
        window: WindowFunction::BlackmanHarris2,
    };

    let chunk_size = 1024;
    let mut resampler = SincFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        2.0,
        params,
        chunk_size,
        channels,
    )
    .map_err(|e| format!("Failed to create resampler: {}", e))?;

    let channel_buffers: Vec<Vec<f32>> = (0..channels)
        .map(|ch| {
            samples
                .iter()
                .skip(ch)
                .step_by(channels)
                .copied()
                .collect()
        })
        .collect();

    let ratio = to_rate as f64 / from_rate as f64;
    let estimated_output_frames = ((frames_in as f64 * ratio) as usize) + chunk_size * 2;
    let mut output_channels: Vec<Vec<f32>> = (0..channels)
        .map(|_| Vec::with_capacity(estimated_output_frames))
        .collect();

    let mut pos = 0;
    let total_frames = frames_in as u32;
    while pos + chunk_size <= frames_in {
        let input_chunk: Vec<Vec<f32>> = channel_buffers
            .iter()
            .map(|ch| ch[pos..pos + chunk_size].to_vec())
            .collect();

        let input_refs: Vec<&[f32]> = input_chunk.iter().map(|v| v.as_slice()).collect();

        let output_chunk = resampler
            .process(&input_refs, None)
            .map_err(|e| format!("Resampling failed: {}", e))?;

        for (ch_idx, ch_out) in output_chunk.iter().enumerate() {
            output_channels[ch_idx].extend_from_slice(ch_out);
        }

        pos += chunk_size;

        if let Some(ref callback) = progress_callback {
            if !callback(pos as u32, total_frames) {
                return Err("Resampling cancelled".to_string());
            }
        }
    }

    let remaining = frames_in - pos;
    if remaining > 0 {
        let input_chunk: Vec<Vec<f32>> = channel_buffers
            .iter()
            .map(|ch| ch[pos..].to_vec())
            .collect();

        let input_refs: Vec<&[f32]> = input_chunk.iter().map(|v| v.as_slice()).collect();

        let output_chunk = resampler
            .process_partial(Some(&input_refs), None)
            .map_err(|e| format!("Resampling partial failed: {}", e))?;

        for (ch_idx, ch_out) in output_chunk.iter().enumerate() {
            output_channels[ch_idx].extend_from_slice(ch_out);
        }
    }

    let flush_output = resampler
        .process_partial::<Vec<f32>>(None, None)
        .map_err(|e| format!("Resampling flush failed: {}", e))?;

    for (ch_idx, ch_out) in flush_output.iter().enumerate() {
        output_channels[ch_idx].extend_from_slice(ch_out);
    }

    let output_frames = output_channels.first().map(|c| c.len()).unwrap_or(0);

    let mut interleaved = Vec::with_capacity(output_frames * channels);
    for frame in 0..output_frames {
        for ch in &output_channels {
            interleaved.push(ch.get(frame).copied().unwrap_or(0.0));
        }
    }

    Ok(interleaved)
}
