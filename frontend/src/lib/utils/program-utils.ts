export function formatTime(seconds: number): string {
	const mins = Math.floor(seconds / 60);
	const secs = (seconds % 60).toFixed(2);
	return `${mins}:${secs.padStart(5, '0')}`;
}

export function getColorStyle(colorName: string): string {
	const colorMap: Record<string, string> = {
		'Red': '#ef4444',
		'Orange': '#f97316',
		'Yellow': '#eab308',
		'Green': '#22c55e',
		'Cyan': '#06b6d4',
		'Blue': '#3b82f6',
		'Purple': '#a855f7',
		'Pink': '#ec4899',
		'White': '#ffffff',
		'Warm': '#fbbf24',
		'Cool': '#93c5fd'
	};
	return colorMap[colorName] || '#e5e5e5';
}

export function sanitizeId(id: string): string {
	return id.replace(/[^a-zA-Z0-9-_]/g, '-');
}

export function dataURLToBlob(dataURL: string): Blob {
	const parts = dataURL.split(',');
	const mime = parts[0].match(/:(.*?);/)?.[1] || 'application/octet-stream';
	const bstr = atob(parts[1]);
	let n = bstr.length;
	const u8arr = new Uint8Array(n);
	while (n--) {
		u8arr[n] = bstr.charCodeAt(n);
	}
	return new Blob([u8arr], { type: mime });
}
