const getApiUrl = () => {
    if (typeof window !== 'undefined') {
        // In the browser, construct the URL to point to port 3010 of the same host.
        return `${window.location.protocol}//${window.location.hostname}:3010/api`;
    }
    // For server-side rendering (SSR) during build, use localhost.
    return 'http://localhost:3010/api';
};

export const API_URL = getApiUrl();
