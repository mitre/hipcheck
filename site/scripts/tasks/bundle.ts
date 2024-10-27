import * as esbuild from "npm:esbuild";
import { denoPlugins } from "jsr:@luca/esbuild-deno-loader";

const footerResult = await esbuild.build({
  plugins: [...denoPlugins()],
  entryPoints: ["src/footer/main.ts"],
  outfile: "../static/js/footer.mjs",
  bundle: true,
  minify: true,
  sourcemap: true,
  format: "esm",
});

for (const warning in footerResult.warnings) {
  console.warn(`footer: ${warning}`);
}

for (const error in footerResult.errors) {
  console.error(`footer: ${error}`);
}

const headerResult = await esbuild.build({
  plugins: [...denoPlugins()],
  entryPoints: ["src/header/main.ts"],
  outfile: "../static/js/header.mjs",
  bundle: true,
  minify: true,
  sourcemap: true,
  format: "esm",
});

for (const warning in headerResult.warnings) {
  console.warn(`header: ${warning}`);
}

for (const error in headerResult.errors) {
  console.error(`header: ${error}`);
}

await esbuild.stop();
