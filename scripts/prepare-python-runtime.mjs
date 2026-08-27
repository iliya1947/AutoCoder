import { createHash } from "node:crypto";
import { createWriteStream, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { get } from "node:https";
import { basename, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { pipeline } from "node:stream/promises";

const version = "3.10.11";
const archiveUrl = `https://www.python.org/ftp/python/${version}/python-${version}-embed-amd64.zip`;
const expectedSha256 = "608619f8619075629c9c69f361352a0da6ed7e62f83a0e19c63e0ea32eb7629d";
const runtimeDirectory = resolve("src-tauri/runtime/python");
const archive = resolve("src-tauri/runtime", basename(archiveUrl));
const targetPlatform = process.env.TAURI_ENV_PLATFORM ?? process.platform;

if (targetPlatform !== "windows" && targetPlatform !== "win32") {
  console.log(`Bundled Python preparation skipped for ${targetPlatform}; Windows is the packaged target.`);
  process.exit(0);
}

mkdirSync(runtimeDirectory, { recursive: true });

async function download(url, destination) {
  await new Promise((resolveDownload, reject) => {
    get(url, (response) => {
      if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
        response.resume();
        download(new URL(response.headers.location, url), destination).then(resolveDownload, reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`Python download failed with HTTP ${response.statusCode}.`));
        return;
      }
      pipeline(response, createWriteStream(destination)).then(resolveDownload, reject);
    }).on("error", reject);
  });
}

if (!existsSync(resolve(runtimeDirectory, "python.exe"))) {
  console.log(`Preparing CPython ${version} embedded runtime for the Windows bundle...`);
  if (!existsSync(archive)) {
    await download(archiveUrl, archive);
  }
  const actualSha256 = createHash("sha256").update(readFileSync(archive)).digest("hex");
  if (actualSha256 !== expectedSha256) {
    rmSync(archive, { force: true });
    throw new Error(`Downloaded Python archive checksum mismatch: ${actualSha256}`);
  }
  const extraction = spawnSync("tar.exe", ["-xf", archive, "-C", runtimeDirectory], {
    encoding: "utf8",
    shell: false,
  });
  rmSync(archive, { force: true });
  if (extraction.status !== 0) {
    throw new Error(`Unable to extract the Python runtime: ${extraction.stderr || extraction.error}`);
  }
}

const pathFile = resolve(runtimeDirectory, "python310._pth");
const pathEntries = readFileSync(pathFile, "utf8")
  .split(/\r?\n/)
  .filter((line) => line && line.trim() !== "../backend" && line.trim() !== "import site");
pathEntries.push("../backend");
writeFileSync(pathFile, `${pathEntries.join("\r\n")}\r\n`, "utf8");

console.log(`Bundled Python runtime is ready at ${runtimeDirectory}.`);
