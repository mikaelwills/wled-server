export type SlotId = 'backing' | 'guide' | 'click' | 'aux';

export interface SlotConfig {
	id: SlotId;
	label: string;
	stereo: boolean;
	color: string;
	waveformColor: string;
	progressColor: string;
	cursorColor: string;
}

export const SLOTS: SlotConfig[] = [
	{
		id: 'backing',
		label: 'Backing',
		stereo: true,
		color: '#a78bfa',
		waveformColor: 'rgba(139, 92, 246, 0.5)',
		progressColor: 'rgba(139, 92, 246, 0.8)',
		cursorColor: 'rgba(167, 139, 250, 0.9)',
	},
	{
		id: 'guide',
		label: 'Guide',
		stereo: false,
		color: '#4ade80',
		waveformColor: 'rgba(34, 197, 94, 0.5)',
		progressColor: 'rgba(34, 197, 94, 0.8)',
		cursorColor: 'rgba(74, 222, 128, 0.9)',
	},
	{
		id: 'click',
		label: 'Click',
		stereo: false,
		color: '#fbbf24',
		waveformColor: 'rgba(251, 191, 36, 0.5)',
		progressColor: 'rgba(251, 191, 36, 0.8)',
		cursorColor: 'rgba(252, 211, 77, 0.9)',
	},
	{
		id: 'aux',
		label: 'Aux',
		stereo: false,
		color: '#60a5fa',
		waveformColor: 'rgba(96, 165, 250, 0.5)',
		progressColor: 'rgba(96, 165, 250, 0.8)',
		cursorColor: 'rgba(147, 197, 253, 0.9)',
	},
];

export function getSlot(id: SlotId): SlotConfig | undefined {
	return SLOTS.find(s => s.id === id);
}

export function getSlotColor(id: SlotId): string {
	return getSlot(id)?.color ?? '#666';
}
