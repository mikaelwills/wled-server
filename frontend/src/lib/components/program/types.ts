export interface Marker {
	id: string;
	time: number;
	label: string;
	boards: string[];
	presetName?: string;
	preset?: number;
	effect?: number;
	color?: string;
	brightness?: number;
	syncRate: number;
}

export interface Board {
	id: string;
	isGroup?: boolean;
	on?: boolean;
	brightness?: number;
	color?: [number, number, number];
}
