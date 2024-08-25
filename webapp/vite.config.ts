import { defineConfig, loadEnv } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { viteSingleFile } from 'vite-plugin-singlefile'

// https://vitejs.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd());
  const nixieHost = `http://${env.VITE_NIXIE_IP}`;
  console.log("proxy to: ", nixieHost);
  return {
    plugins: [svelte(), viteSingleFile()],
    server: {
      proxy: {
        '/config': {
          target: nixieHost,
          changeOrigin: true,
          secure: false,
        },
      },
    },
  };
});
