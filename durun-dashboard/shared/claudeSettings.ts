// コントロール パネルで ~/.claude/settings.json を直すための項目。VS Code の設定の画面のように、公開されている JSON Schema から入力欄を作る。

/** json は、オブジェクトや配列など、入力欄 1 つでは表せない値。JSON の文字で直す。 */
export type ClaudeSettingKind = 'boolean' | 'enum' | 'string' | 'number' | 'json';
export type ClaudeSettingField = { key: string; description: string; kind: ClaudeSettingKind; options?: string[] };
/** fields は直せる項目、values は settings.json の今の中身。 */
export type ClaudeSettings = { fields: ClaudeSettingField[]; values: Record<string, unknown> };

type SchemaProp = { type?: string | string[]; enum?: unknown[]; description?: string };

// 組織の管理者が配る設定と、使わなくなった設定は、本人の settings.json に書いても効かないので並べない。
const NOT_FOR_USER = /^\((Managed|Admin|Windows managed)|^DEPRECATED/;

const kindOf = (p: SchemaProp): ClaudeSettingKind => {
  if (p.enum?.every((v) => typeof v === 'string')) return 'enum';
  if (p.type === 'boolean') return 'boolean';
  if (p.type === 'integer' || p.type === 'number') return 'number';
  if (p.type === 'string') return 'string';
  return 'json';
};

/** スキーマの最上位の項目から、本人が直せるものを、名前の順に並べる。 */
export const claudeSettingFields = (schema: { properties?: Record<string, SchemaProp> }): ClaudeSettingField[] =>
  Object.entries(schema.properties ?? {})
    .filter(([key, p]) => key !== '$schema' && !NOT_FOR_USER.test(p.description ?? ''))
    .map(([key, p]) => ({ key, description: p.description ?? '', kind: kindOf(p), ...(kindOf(p) === 'enum' ? { options: p.enum as string[] } : {}) }))
    .sort((a, b) => a.key.localeCompare(b.key));

/**
 * settings.json の中身を読む。Claude Code は ] と } の手前のカンマを許すので、文字列の外にあるものは外してから読む。
 * 読めなければ、書き換えて壊さないよう投げる。
 */
export const parseClaudeSettings = (text: string): Record<string, unknown> =>
  JSON.parse(text.replace(/("(?:\\.|[^"\\])*")|,(\s*[\]}])/g, (_, str: string | undefined, close: string) => str ?? close)) as Record<string, unknown>;
