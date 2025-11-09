import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),

	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			fallback: 'index.html',
			precompress: false,
			strict: true
		})
	},

	compilerOptions: {
		// Disable a11y warnings (for personal project)
		warningFilter: (warning) => {
			// Filter out accessibility warnings
			if (warning.code.startsWith('a11y_')) return false;
			// Show all other warnings including unused CSS
			return true;
		}
	}
};

export default config;
