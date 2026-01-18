import type WaveSurfer from 'wavesurfer.js';

export interface GridPosition {
	time: number;
	isDownbeat: boolean;
}

export function calculateGridInterval(bpm: number, gridMultiplier: number): number {
	const beatInterval = 60 / bpm;
	const barInterval = beatInterval * 4;
	return (barInterval * 4) / gridMultiplier;
}

export function calculateSnapThreshold(
	wavesurfer: WaveSurfer,
	zoomLevel: number,
	audioDuration: number,
	thresholdPx: number = 25
): number {
	const wrapper = wavesurfer.getWrapper();
	if (!wrapper) return 0;
	const pixelsPerSecond = zoomLevel > 0 ? zoomLevel : wrapper.clientWidth / audioDuration;
	return thresholdPx / pixelsPerSecond;
}

export function snapToGrid(
	time: number,
	bpm: number | null,
	gridOffset: number,
	gridMultiplier: number,
	wavesurfer: WaveSurfer | null,
	zoomLevel: number,
	audioDuration: number | null
): number {
	if (!bpm || bpm <= 0 || !wavesurfer || !audioDuration) return time;

	const gridInterval = calculateGridInterval(bpm, gridMultiplier);
	const snapThresholdTime = calculateSnapThreshold(wavesurfer, zoomLevel, audioDuration);

	const relativeTime = time - gridOffset;
	const nearestGridRelative = Math.round(relativeTime / gridInterval) * gridInterval;
	const nearestGridTime = gridOffset + nearestGridRelative;

	if (Math.abs(time - nearestGridTime) <= snapThresholdTime) {
		return nearestGridTime;
	}
	return time;
}

export function generateGridPositions(
	bpm: number,
	gridOffset: number,
	gridMultiplier: number,
	audioDuration: number
): GridPosition[] {
	const beatInterval = 60 / bpm;
	const barInterval = beatInterval * 4;
	const gridInterval = calculateGridInterval(bpm, gridMultiplier);

	const positions: GridPosition[] = [];

	for (let t = gridOffset; t <= audioDuration; t += gridInterval) {
		if (t >= 0) {
			const relativeTime = t - gridOffset;
			const barNumber = Math.round(relativeTime / barInterval);
			positions.push({ time: t, isDownbeat: barNumber % 4 === 0 });
		}
	}

	for (let t = gridOffset - gridInterval; t >= 0; t -= gridInterval) {
		const relativeTime = t - gridOffset;
		const barNumber = Math.round(relativeTime / barInterval);
		positions.push({ time: t, isDownbeat: barNumber % 4 === 0 });
	}

	return positions;
}
