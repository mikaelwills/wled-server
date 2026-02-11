<script lang="ts">
	import { API_URL } from '$lib/api';
	import { SLOTS, getSlotColor, type SlotId } from '$lib/slots';

	interface Props {
		open: boolean;
		onClose: () => void;
	}

	let { open, onClose }: Props = $props();

	interface Routing {
		device_id: string;
		backing_left: number;
		backing_right: number;
		guide: number;
		click: number;
	}

	let routing: Routing | null = $state(null);
	let outputChannels = $state(0);
	let deviceId: string | null = $state(null);
	let loading = $state(true);
	let saving = $state(false);
	let selectedSlot: SlotId | null = $state(null);

	function getSlotChannel(slot: SlotId): number | null {
		if (!routing) return null;
		switch (slot) {
			case 'backing': return routing.backing_left;
			case 'guide': return routing.guide;
			case 'click': return routing.click;
			case 'aux': return null;
		}
	}

	function getSlotChannels(slot: SlotId): number[] {
		if (!routing) return [];
		switch (slot) {
			case 'backing': return [routing.backing_left, routing.backing_right];
			case 'guide': return [routing.guide];
			case 'click': return [routing.click];
			case 'aux': return [];
		}
	}

	function setSlotChannel(slot: SlotId, channel: number) {
		if (!routing) return;
		switch (slot) {
			case 'backing':
				routing.backing_left = channel;
				routing.backing_right = channel < outputChannels ? channel + 1 : channel;
				break;
			case 'guide':
				routing.guide = channel;
				break;
			case 'click':
				routing.click = channel;
				break;
			case 'aux':
				break;
		}
		routing = { ...routing };
	}

	async function fetchData() {
		loading = true;
		try {
			const [settingsRes, devicesRes] = await Promise.all([
				fetch(`${API_URL}/audio/settings`),
				fetch(`${API_URL}/audio/devices`)
			]);

			if (settingsRes.ok) {
				const settings = await settingsRes.json();
				deviceId = settings.preferred_device_id;
			}

			if (devicesRes.ok && deviceId) {
				const devices = await devicesRes.json();
				const selected = devices.find((d: any) => d.id === deviceId);
				if (selected) {
					outputChannels = selected.output_channels;
				}
			}

			if (!deviceId) {
				loading = false;
				return;
			}

			const routingRes = await fetch(`${API_URL}/audio/routing/${encodeURIComponent(deviceId)}`);
			if (routingRes.ok) {
				routing = await routingRes.json();
			}
		} catch (e) {
			console.error('Failed to fetch routing data:', e);
		}
		loading = false;
	}

	async function saveRouting() {
		if (!deviceId || !routing) return;
		saving = true;
		try {
			const res = await fetch(`${API_URL}/audio/routing/${encodeURIComponent(deviceId)}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({
					backing_left: routing.backing_left,
					backing_right: routing.backing_right,
					guide: routing.guide,
					click: routing.click
				})
			});
			if (res.ok) {
				onClose();
			}
		} catch (e) {
			console.error('Failed to save routing:', e);
		}
		saving = false;
	}

	function selectChannel(channel: number) {
		if (!selectedSlot) return;
		setSlotChannel(selectedSlot, channel);
	}

	function getChannelSlot(channel: number): SlotId | null {
		if (!routing) return null;
		if (channel === routing.backing_left || channel === routing.backing_right) return 'backing';
		if (channel === routing.guide) return 'guide';
		if (channel === routing.click) return 'click';
		return null;
	}

	
	$effect(() => {
		if (open) {
			selectedSlot = null;
			fetchData();
		}
	});
</script>

{#if open}
	<div class="modal-overlay" onclick={onClose}>
		<div class="modal-content" onclick={(e) => e.stopPropagation()}>
			<div class="modal-header">
				<h3>{#if deviceId && outputChannels > 2}{deviceId} • {outputChannels}ch{:else}Channel Routing{/if}</h3>
				<button class="close-btn" onclick={onClose}>×</button>
			</div>

			{#if loading}
				<div class="modal-body">
					<p class="loading-text">Loading...</p>
				</div>
			{:else if !deviceId}
				<div class="modal-body">
					<p class="no-device-text">No audio device selected</p>
					<p class="hint-text">Select a device in Settings first</p>
				</div>
			{:else if outputChannels <= 2}
				<div class="modal-body">
					<p class="stereo-text">Stereo device</p>
					<p class="hint-text">All tracks mix to stereo output</p>
				</div>
			{:else}
				<div class="modal-body">
					<div class="slots-list">
						{#each SLOTS as slot}
							{@const channels = getSlotChannels(slot.id)}
							{@const isSelected = selectedSlot === slot.id}
							<button
								class="slot-row"
								class:selected={isSelected}
								style="--slot-color: {slot.color}"
								onclick={() => selectedSlot = isSelected ? null : slot.id}
							>
								<span class="slot-label">{slot.label}</span>
								<span class="slot-channels">
									{#if channels.length === 0}
										—
									{:else if slot.stereo && channels.length === 2}
										{channels[0]}-{channels[1]}
									{:else}
										{channels[0]}
									{/if}
								</span>
							</button>
						{/each}
					</div>

					{#if selectedSlot}
						{@const slotInfo = SLOTS.find(s => s.id === selectedSlot)}
						<div class="channel-picker">
							<p class="picker-hint">
								{slotInfo?.stereo ? 'Select left channel (stereo pair)' : 'Select channel'}
							</p>
							<div class="channels-grid">
								{#each Array(outputChannels) as _, i}
									{@const channelNum = i + 1}
									{@const ownerSlot = getChannelSlot(channelNum)}
									{@const isOwned = ownerSlot === selectedSlot}
									<button
										class="channel-btn"
										class:selected={isOwned}
										style={ownerSlot ? `--owner-color: ${getSlotColor(ownerSlot)}` : ''}
										class:has-owner={ownerSlot && ownerSlot !== selectedSlot}
										onclick={() => selectChannel(channelNum)}
									>
										{channelNum}
									</button>
								{/each}
							</div>
						</div>
					{/if}
				</div>
				<div class="modal-footer">
					<button class="save-btn" onclick={saveRouting} disabled={saving}>
						{saving ? 'Saving...' : 'Save'}
					</button>
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.modal-overlay {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background: rgba(0, 0, 0, 0.8);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.modal-content {
		background: #0c0c0c;
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 12px;
		min-width: 340px;
		max-width: 90vw;
		max-height: 80vh;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1rem 1.25rem;
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	.modal-header h3 {
		margin: 0;
		font-size: 1rem;
		font-weight: 500;
		color: #e5e5e5;
	}

	.close-btn {
		background: transparent;
		border: none;
		color: #666;
		font-size: 1.5rem;
		cursor: pointer;
		padding: 0;
		line-height: 1;
	}

	.close-btn:hover {
		color: #999;
	}

	.modal-body {
		padding: 1.25rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}

	.loading-text,
	.no-device-text,
	.stereo-text {
		text-align: center;
		color: #888;
		font-size: 0.9rem;
	}

	.hint-text {
		text-align: center;
		color: #555;
		font-size: 0.75rem;
		margin: 0;
	}

	.slots-list {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.slot-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 1rem;
		background: #111;
		border: 1px solid rgba(255, 255, 255, 0.05);
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.15s;
	}

	.slot-row:hover {
		background: #161616;
		border-color: rgba(255, 255, 255, 0.1);
	}

	.slot-row.selected {
		background: rgba(255, 255, 255, 0.05);
		border-color: rgba(255, 255, 255, 0.15);
	}

	.slot-label {
		color: var(--slot-color);
		font-size: 0.875rem;
		font-weight: 500;
	}

	.slot-channels {
		color: var(--slot-color);
		font-size: 0.875rem;
		font-family: 'SF Mono', 'Monaco', 'Consolas', monospace;
	}

	.channel-picker {
		margin-top: 0.5rem;
		padding-top: 1rem;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
	}

	.picker-hint {
		text-align: center;
		color: #555;
		font-size: 0.75rem;
		margin: 0 0 0.75rem 0;
	}

	.channels-grid {
		display: grid;
		grid-template-columns: repeat(6, 1fr);
		gap: 0.5rem;
	}

	.channel-btn {
		aspect-ratio: 1;
		background: #1a1a1a;
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 8px;
		color: #666;
		font-size: 0.9rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
	}

	.channel-btn:hover {
		background: #222;
		color: #888;
		border-color: rgba(255, 255, 255, 0.2);
	}

	.channel-btn.selected {
		background: rgba(255, 255, 255, 0.1);
		border-color: var(--owner-color, #a78bfa);
		color: var(--owner-color, #a78bfa);
	}

	.channel-btn.has-owner {
		background: rgba(255, 255, 255, 0.03);
		border-color: color-mix(in srgb, var(--owner-color) 40%, transparent);
		color: color-mix(in srgb, var(--owner-color) 60%, #666);
	}

	.modal-footer {
		padding: 1rem 1.25rem 1.25rem;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
	}

	.save-btn {
		width: 100%;
		padding: 0.75rem;
		background: transparent;
		border: 1px solid #333;
		border-radius: 8px;
		color: #888;
		font-size: 0.875rem;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
	}

	.save-btn:hover:not(:disabled) {
		background: #161616;
		border-color: #444;
		color: #ccc;
	}

	.save-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
