// frontend/src/lib/loopy-db.ts
// All Loopy Pro related backend operations
import { browser } from '$app/environment';
import { get } from 'svelte/store';
import { loopyProSettings, loopyProSettingsLoading, loopyProSettingsError } from './store';
import { API_URL } from '$lib/api';
import type { LoopyProSettings } from './store';

/**
 * Initialize Loopy Pro settings from API
 */
export async function initLoopyProSettings(): Promise<void> {
	if (!browser) return;

	loopyProSettingsLoading.set(true);
	loopyProSettingsError.set(null);

	try {
		const response = await fetch(`${API_URL}/settings/loopy-pro`);

		if (!response.ok) {
			throw new Error('Failed to load Loopy Pro settings from server');
		}

		const data = await response.json();
		loopyProSettings.set(data);
		loopyProSettingsLoading.set(false);
	} catch (error) {
		console.error('Failed to load Loopy Pro settings:', error);
		loopyProSettingsError.set('Failed to load Loopy Pro settings from server.');
		loopyProSettingsLoading.set(false);
		// Keep default values in store on error
	}
}

/**
 * Update Loopy Pro settings and persist to API
 */
export async function updateLoopyProSettings(settings: LoopyProSettings): Promise<void> {
	if (!browser) return;

	try {
		const response = await fetch(`${API_URL}/settings/loopy-pro`, {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(settings)
		});

		if (!response.ok) {
			throw new Error(`Failed to update Loopy Pro settings: ${response.statusText}`);
		}

		// Update local store
		loopyProSettings.set(settings);
	} catch (error) {
		console.error('Failed to update Loopy Pro settings:', error);
		loopyProSettingsError.set('Failed to update Loopy Pro settings on server.');
		throw error;
	}
}

/**
 * Send OSC message to Loopy Pro using settings from store
 */
export async function sendOSC(address: string): Promise<void> {
	if (!browser) return;

	// Get current settings from store (no API call needed)
	const settings: LoopyProSettings = get(loopyProSettings);

	try {
		const response = await fetch(`${API_URL}/osc`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({
				address,
				ip: settings.ip,
				port: settings.port
			})
		});

		if (!response.ok) {
			throw new Error(`Failed to send OSC message: ${response.statusText}`);
		}
	} catch (error) {
		console.error('Failed to send OSC message:', error);
		throw error;
	}
}
