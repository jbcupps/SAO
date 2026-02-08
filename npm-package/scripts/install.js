#!/usr/bin/env node
"use strict";

const https = require("https");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const { createGunzip } = require("zlib");

const pkg = require("../package.json");
const version = pkg.version;

const REPO = "jbcupps/SAO";
const BIN_DIR = path.join(__dirname, "..", "bin");

const PLATFORM_MAP = {
  "linux-x64": "sao-server-linux-x64.tar.gz",
  "darwin-x64": "sao-server-macos-x64.tar.gz",
  "darwin-arm64": "sao-server-macos-arm64.tar.gz",
  "win32-x64": "sao-server-windows-x64.zip",
};

function getPlatformKey() {
  const platform = process.platform;
  const arch = process.arch;
  const key = `${platform}-${arch}`;

  if (!PLATFORM_MAP[key]) {
    console.error(`Unsupported platform: ${platform}-${arch}`);
    console.error(`Supported platforms: ${Object.keys(PLATFORM_MAP).join(", ")}`);
    process.exit(1);
  }

  return key;
}

function download(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (res) => {
        // Follow redirects (GitHub releases redirect to S3)
        if (res.statusCode === 301 || res.statusCode === 302) {
          return download(res.headers.location).then(resolve).catch(reject);
        }

        if (res.statusCode !== 200) {
          reject(new Error(`Download failed: HTTP ${res.statusCode} from ${url}`));
          return;
        }

        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

function extractTarGz(buffer, destDir) {
  // Write to a temp file and use tar to extract
  const tmpFile = path.join(destDir, "_tmp_archive.tar.gz");
  fs.writeFileSync(tmpFile, buffer);

  try {
    execSync(`tar xzf "${tmpFile}" -C "${destDir}"`, { stdio: "pipe" });
  } finally {
    fs.unlinkSync(tmpFile);
  }
}

function extractZip(buffer, destDir) {
  // Write to a temp file and use PowerShell to extract
  const tmpFile = path.join(destDir, "_tmp_archive.zip");
  fs.writeFileSync(tmpFile, buffer);

  try {
    if (process.platform === "win32") {
      execSync(
        `powershell -Command "Expand-Archive -Path '${tmpFile}' -DestinationPath '${destDir}' -Force"`,
        { stdio: "pipe" }
      );
    } else {
      execSync(`unzip -o "${tmpFile}" -d "${destDir}"`, { stdio: "pipe" });
    }
  } finally {
    fs.unlinkSync(tmpFile);
  }
}

async function main() {
  const platformKey = getPlatformKey();
  const filename = PLATFORM_MAP[platformKey];
  const url = `https://github.com/${REPO}/releases/download/v${version}/${filename}`;

  console.log(`Downloading sao-server v${version} for ${platformKey}...`);
  console.log(`  ${url}`);

  // Ensure bin directory exists
  if (!fs.existsSync(BIN_DIR)) {
    fs.mkdirSync(BIN_DIR, { recursive: true });
  }

  let buffer;
  try {
    buffer = await download(url);
  } catch (err) {
    console.error(`Failed to download sao-server binary: ${err.message}`);
    console.error(
      "You can manually download the binary from:",
      `https://github.com/${REPO}/releases/tag/v${version}`
    );
    process.exit(1);
  }

  console.log(`Extracting to ${BIN_DIR}...`);

  if (filename.endsWith(".tar.gz")) {
    extractTarGz(buffer, BIN_DIR);
  } else if (filename.endsWith(".zip")) {
    extractZip(buffer, BIN_DIR);
  }

  // Set executable permissions on Linux/macOS
  if (process.platform !== "win32") {
    const binaryPath = path.join(BIN_DIR, "sao-server");
    if (fs.existsSync(binaryPath)) {
      fs.chmodSync(binaryPath, 0o755);
      console.log(`Set executable permissions on ${binaryPath}`);
    }
  }

  // Verify the binary exists
  const binaryName = process.platform === "win32" ? "sao-server.exe" : "sao-server";
  const finalPath = path.join(BIN_DIR, binaryName);

  if (fs.existsSync(finalPath)) {
    console.log(`sao-server v${version} installed successfully at ${finalPath}`);
  } else {
    console.error(`Binary not found at expected path: ${finalPath}`);
    process.exit(1);
  }
}

main();
