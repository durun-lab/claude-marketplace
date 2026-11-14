// コントロール パネルから ~/.claude/settings.json を読み書きする。直した項目のほかは、書いてあったまま残す。
import fs from 'node:fs';
import path from 'node:path';
import { type ClaudeSettingField, claudeSettingFields } from '../shared/claudeSettings.ts';
import { cached } from './cache.ts';
import { CLAUDE_DIR } from './claude.ts';

const FILE = path.join(CLAUDE_DIR, 'settings.json');
// Claude Code の公式ドキュメントが案内している、settings.json の JSON Schema。
const SCHEMA_URL = 'https://www.schemastore.org/claude-code-settings.json';
// 項目が増えるのは Claude Code の更新のときだけなので、1 日ごとに読み直す。
const SCHEMA_MAX_AGE = 24 * 60 * 60_000;

/** settings.json に書ける項目。 */
export const claudeSettingsFields = (fresh = false): Promise<ClaudeSettingField[]> =>
  cached('claude-settings-fields-v1', { maxAge: SCHEMA_MAX_AGE, fresh }, async () => {
    const res = await fetch(SCHEMA_URL);
    if (!res.ok) throw new Error(`settings.json のスキーマを読めません（HTTP ${res.status}）`);
    return claudeSettingFields((await res.json()) as Parameters<typeof claudeSettingFields>[0]);
  });

/** 今の中身。読めない JSON は、書き換えて壊さないよう、理由を返す。 */
export const readClaudeSettings = (): Record<string, unknown> =>
  fs.existsSync(FILE) ? (JSON.parse(fs.readFileSync(FILE, 'utf8')) as Record<string, unknown>) : {};

/** key の値を value にする。value が undefined なら、項目を消して既定に戻す。 */
export const writeClaudeSetting = (key: string, value: unknown): void => {
  const data = readClaudeSettings();
  if (value === undefined) delete data[key];
  else data[key] = value;
  fs.mkdirSync(CLAUDE_DIR, { recursive: true });
  fs.writeFileSync(FILE, `${JSON.stringify(data, null, 2)}\n`);
};
