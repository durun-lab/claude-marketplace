// pnpm start の本体。画面のバンドルを esbuild で作り直し続けながら、サーバーを node --watch で動かす。
// pnpm は Windows ではスクリプトを cmd.exe で動かし、sh の trap や ${VAR:-…} が使えないので、node から 2 つを起動する。
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import * as esbuild from 'esbuild';
import { builds } from './build.ts';
import { DATA_DIR } from './store.ts';

const ROOT = path.join(import.meta.dirname, '..');

// esbuild はこのプロセスの中で見張るので、サーバーが落ちても esbuild だけが残ることはない。
const contexts = await Promise.all(builds.map((b) => esbuild.context({ ...b, logLevel: 'info' })));
await Promise.all(contexts.map((c) => c.watch()));

// node --watch は --env-file-if-exists のファイルが無くてもディレクトリを見張るので、初めて起動するときに無いと落ちる。
fs.mkdirSync(DATA_DIR, { recursive: true });
const server = spawn(process.execPath, [`--env-file-if-exists=${path.join(DATA_DIR, '.env.local')}`, '--watch', path.join(ROOT, 'server', 'main.ts')], { cwd: ROOT, stdio: 'inherit' });
server.on('exit', async (code) => {
  await Promise.all(contexts.map((c) => c.dispose()));
  process.exit(code ?? 0);
});
