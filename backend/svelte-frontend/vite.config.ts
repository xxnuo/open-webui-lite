import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

import { viteStaticCopy } from 'vite-plugin-static-copy';

export default defineConfig({
	plugins: [
		sveltekit(),
		viteStaticCopy({
			targets: [
				{
					src: 'node_modules/onnxruntime-web/dist/*.jsep.*',

					dest: 'wasm'
				}
			]
		})
	],
	define: {
		APP_VERSION: JSON.stringify(process.env.npm_package_version),
		APP_BUILD_HASH: JSON.stringify(process.env.APP_BUILD_HASH || 'dev-build')
	},
	build: {
		sourcemap: true
	},
	worker: {
		format: 'es'
	},
	esbuild: {
		pure: process.env.ENV === 'dev' ? [] : ['console.log', 'console.debug', 'console.error']
	},
	ssr: {
		noExternal: ['@sveltejs/kit']
	},
	server: {
		proxy: {
			'/api': {
				target: 'http://localhost:8168',
				changeOrigin: true
			},
			'/socket.io': {
				target: 'http://localhost:8168',
				changeOrigin: true,
				ws: true
			}
		}
	},
	optimizeDeps: {
		include: [
			'codemirror',
			'@codemirror/view',
			'@codemirror/state',
			'@codemirror/language',
			'@codemirror/language-data',
			'@codemirror/autocomplete',
			'@codemirror/commands',
			'@codemirror/theme-one-dark',
			'codemirror-lang-hcl',
			'codemirror-lang-elixir'
		],
		exclude: []
	}
});
