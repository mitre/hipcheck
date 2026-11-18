import { defineConfig } from '@hey-api/openapi-ts';

export default defineConfig({
  input: "../backend/openapi/nv-server-openapi.json", 
  output: './src/lib/api/generated',
  plugins: [
    '@hey-api/typescript',
    '@hey-api/client-fetch'
  ]
});