// Dynamic API URL that automatically uses the current browser hostname
// Works in dev (localhost), network access (192.168.x.x), and Docker deployment
export const API_URL = typeof window !== 'undefined'
    ? `${window.location.protocol}//${window.location.hostname}:3010`
    : 'http://localhost:3010'; // Fallback for SSR
