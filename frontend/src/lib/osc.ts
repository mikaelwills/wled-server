import { API_URL } from './api';

export async function sendOSC(address: string) {
	await fetch(`${API_URL}/osc`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ address })
	});
}
