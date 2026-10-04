import { defineConfig } from 'vite'
import { devtools } from '@tanstack/devtools-vite'

import { tanstackStart } from '@tanstack/react-start/plugin/vite'

import viteReact from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

const config = defineConfig({
	// A stray console.log of a customer record would otherwise ship to
	// production. This is the net; the calls themselves were removed.
	esbuild: { drop: ['console', 'debugger'] },
	resolve: { tsconfigPaths: true },
	plugins: [
		devtools(),
		tailwindcss(),
		tanstackStart({ spa: { enabled: true } }),
		viteReact(),
	],
})

export default config
