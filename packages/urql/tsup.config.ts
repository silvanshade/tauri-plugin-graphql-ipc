import * as tsup from "tsup";

export default tsup.defineConfig({
  entry: ["src/index.ts"],
  format: ["esm"],
  outDir: "dist",
  clean: true,
  dts: true,
});
