import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { parseEnv } from 'node:util';
import { readEnvValues, writeEnvValue } from '../server/envFile.ts';
import { tempDir } from './tmp.ts';

test('.env.local の行を書き換え、無ければ足し、空にしたら消して、注釈とほかの行は残す。secret の値は返さない', () => {
  const file = path.join(tempDir('durun-dashboard-env-'), '.env.local');
  fs.writeFileSync(file, '# 注釈\nJIRA_BASE_URL=https://old.example\nOTHER=1\n');
  writeEnvValue(file, 'JIRA_BASE_URL', 'https://new.example');
  writeEnvValue(file, 'JIRA_API_TOKEN', 'secret-token');
  writeEnvValue(file, 'TERMINAL_ATTACH_COMMAND', 'tmux new-session -A -s "$(basename "$PWD")" # 注釈ではない');
  assert.equal(fs.readFileSync(file, 'utf8').split('\n')[0], '# 注釈');
  const parsed = parseEnv(fs.readFileSync(file, 'utf8'));
  assert.deepEqual([parsed.JIRA_BASE_URL, parsed.OTHER, parsed.JIRA_API_TOKEN], ['https://new.example', '1', 'secret-token']);
  assert.equal(parsed.TERMINAL_ATTACH_COMMAND, 'tmux new-session -A -s "$(basename "$PWD")" # 注釈ではない');
  const values = readEnvValues(file);
  assert.deepEqual(values.JIRA_API_TOKEN, { set: true });
  assert.deepEqual(values.JIRA_BASE_URL, { set: true, value: 'https://new.example' });

  writeEnvValue(file, 'JIRA_BASE_URL', '');
  assert.equal(parseEnv(fs.readFileSync(file, 'utf8')).JIRA_BASE_URL, undefined);
  assert.equal(fs.statSync(file).mode & 0o777, 0o600);
  assert.throws(() => writeEnvValue(file, 'PATH', '/bin'), /設定できません/);
  assert.throws(() => writeEnvValue(file, 'NOTION_TOKEN', 'a\nb'), /改行/);
});
