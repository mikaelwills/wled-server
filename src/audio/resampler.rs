use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction, Resampler};

pub fn resample(
    samples: &[f32],
    channels: u16,
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, String> {
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
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Cubic,
        oversampling_factor: 256,
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
