// 環境によって動作が異なるためmkdir -pのラッパースクリプトを用意する
import { mkdir } from "fs/promises";
import path from "path";

async function main() {
  const target = process.argv[2];

  if (!target) {
    console.error("Usage: mkdirp <directory>");
    process.exit(1);
  }

  const fullPath = path.resolve(target);

  try {
    await mkdir(fullPath, { recursive: true });
    console.log(`Created: ${fullPath}`);
  } catch (err) {
    console.error(`Failed to create directory: ${fullPath}`);
    console.error(err);
    process.exit(1);
  }
}

main();
