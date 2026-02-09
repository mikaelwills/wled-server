<script lang="ts">
	import { onMount } from 'svelte';

	export let color: [number, number, number] = [255, 255, 255];
	export let onColorChange: (r: number, g: number, b: number) => void;
	export let disabled: boolean = false;

	let canvas: HTMLCanvasElement;
	let isDragging = false;
	const size = 200;
	const centerX = size / 2;
	const centerY = size / 2;
	const radius = size / 2 - 10;

	function rgbToHsv(r: number, g: number, b: number): [number, number, number] {
		r /= 255;
		g /= 255;
		b /= 255;
		const max = Math.max(r, g, b);
		const min = Math.min(r, g, b);
		const delta = max - min;

		let h = 0;
		if (delta !== 0) {
			if (max === r) h = ((g - b) / delta + (g < b ? 6 : 0)) / 6;
			else if (max === g) h = ((b - r) / delta + 2) / 6;
			else h = ((r - g) / delta + 4) / 6;
		}

		const s = max === 0 ? 0 : delta / max;
		const v = max;

		return [h * 360, s * 100, v * 100];
	}

	function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
		h /= 360;
		s /= 100;
		v /= 100;

		const i = Math.floor(h * 6);
		const f = h * 6 - i;
		const p = v * (1 - s);
		const q = v * (1 - f * s);
		const t = v * (1 - (1 - f) * s);

		let r = 0,
			g = 0,
			b = 0;
		switch (i % 6) {
			case 0:
				(r = v), (g = t), (b = p);
				break;
			case 1:
				(r = q), (g = v), (b = p);
				break;
			case 2:
				(r = p), (g = v), (b = t);
				break;
			case 3:
				(r = p), (g = q), (b = v);
				break;
			case 4:
				(r = t), (g = p), (b = v);
				break;
			case 5:
				(r = v), (g = p), (b = q);
				break;
		}

		return [Math.round(r * 255), Math.round(g * 255), Math.round(b * 255)];
	}

	function drawColorWheel() {
		if (!canvas) return;
		const ctx = canvas.getContext('2d');
		if (!ctx) return;

		ctx.clearRect(0, 0, size, size);

		// Draw color wheel
		for (let angle = 0; angle < 360; angle += 1) {
			for (let r = 0; r < radius; r += 1) {
				const sat = (r / radius) * 100;
				const [red, green, blue] = hsvToRgb(angle, sat, 100);

				ctx.fillStyle = `rgb(${red}, ${green}, ${blue})`;
				const x = centerX + r * Math.cos((angle * Math.PI) / 180);
				const y = centerY + r * Math.sin((angle * Math.PI) / 180);
				ctx.fillRect(x, y, 2, 2);
			}
		}

		// Draw current color indicator
		const [h, s] = rgbToHsv(color[0], color[1], color[2]);
		const indicatorRadius = (s / 100) * radius;
		const indicatorX = centerX + indicatorRadius * Math.cos((h * Math.PI) / 180);
		const indicatorY = centerY + indicatorRadius * Math.sin((h * Math.PI) / 180);

		ctx.strokeStyle = '#ffffff';
		ctx.lineWidth = 3;
		ctx.beginPath();
		ctx.arc(indicatorX, indicatorY, 8, 0, 2 * Math.PI);
		ctx.stroke();

		ctx.strokeStyle = '#000000';
		ctx.lineWidth = 1;
		ctx.beginPath();
		ctx.arc(indicatorX, indicatorY, 8, 0, 2 * Math.PI);
		ctx.stroke();
	}

	function handleClick(e: MouseEvent) {
		if (disabled) return;

		const rect = canvas.getBoundingClientRect();
		const x = e.clientX - rect.left;
		const y = e.clientY - rect.top;

		const dx = x - centerX;
		const dy = y - centerY;
		const distance = Math.sqrt(dx * dx + dy * dy);

		if (distance > radius) return;

		let angle = (Math.atan2(dy, dx) * 180) / Math.PI;
		if (angle < 0) angle += 360;

		const saturation = Math.min((distance / radius) * 100, 100);
		const [r, g, b] = hsvToRgb(angle, saturation, 100);

		onColorChange(r, g, b);
	}

	function handleMouseDown(e: MouseEvent) {
		if (disabled) return;
		isDragging = true;
		handleClick(e);
	}

	function handleMouseMove(e: MouseEvent) {
		if (isDragging) {
			handleClick(e);
		}
	}

	function handleMouseUp() {
		isDragging = false;
	}

	onMount(() => {
		drawColorWheel();
	});

	$: if (canvas && color) {
		drawColorWheel();
	}
</script>

<canvas
	bind:this={canvas}
	width={size}
	height={size}
	class:disabled
	on:mousedown={handleMouseDown}
	on:mousemove={handleMouseMove}
	on:mouseup={handleMouseUp}
	on:mouseleave={handleMouseUp}
	on:touchstart={(e) => {
		e.preventDefault();
		const touch = e.touches[0];
		handleMouseDown(new MouseEvent('mousedown', { clientX: touch.clientX, clientY: touch.clientY }));
	}}
	on:touchmove={(e) => {
		e.preventDefault();
		const touch = e.touches[0];
		handleMouseMove(new MouseEvent('mousemove', { clientX: touch.clientX, clientY: touch.clientY }));
	}}
	on:touchend={handleMouseUp}
></canvas>

<style>
	canvas {
		cursor: crosshair;
		border-radius: 50%;
		touch-action: none;
	}

	canvas.disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
</style>
