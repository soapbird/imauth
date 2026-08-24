import { copyFile, mkdir, readdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const source = fileURLToPath(new URL("../../../proto/imauth/v1/", import.meta.url));
const target = fileURLToPath(new URL("../proto/imauth/v1/", import.meta.url));

await rm(target, { recursive: true, force: true });
await mkdir(target, { recursive: true });

for (const entry of await readdir(source)) {
  if (entry.endsWith(".proto")) {
    await copyFile(`${source}/${entry}`, `${target}/${entry}`);
  }
}
