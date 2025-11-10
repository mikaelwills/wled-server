import { API_URL } from './api';

export async function sendOSC(address: string) {
	// Fetch current settings from server
	const settingsResponse = await fetch(`${API_URL}/settings/loopy-pro`);
	const settings = await settingsResponse.json();

	await fetch(`${API_URL}/osc`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			address,
			ip: settings.ip,
			port: settings.port
		})
	});
}
