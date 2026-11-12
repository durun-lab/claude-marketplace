// コントロール パネルから .env.local を読み書きする。人間が書いた注釈や、ここで扱わない行は書いたまま残す。
// サーバーは node --watch で .env.local を見張っているので、書き換えると起動し直して新しい値を読む。
import fs from 'node:fs';
import { parseEnv } from 'node:util';
import { ENV_SETTINGS, type EnvValues } from '../shared/envSettings.ts';

const read = (file: string): string => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : '');

/** 今の値。secret の項目は、設定してあるかだけを返す。 */
export const readEnvValues = (file: string): EnvValues => {
  const parsed = parseEnv(read(file));
  return Object.fromEntries(ENV_SETTINGS.map(({ key, secret }) => [key, { set: !!parsed[key], ...(secret ? {} : { value: parsed[key] ?? '' }) }]));
};

// node の .env の読み方では、引用符で始まる値は引用符の中だけを読み、# の後は注釈になる。そのときだけ、値を単引用符で囲む。
const lineOf = (key: string, value: string): string => (/^['"`]|#/.test(value) ? `${key}='${value}'` : `${key}=${value}`);

/**
 * key の行を value に書き換える。行が無ければ末尾に足し、value が空なら行を消して既定に戻す。
 * 改行と、単引用符で囲めない値は書かずに理由を返す。トークンを含むので、本人だけが読めるファイルにする。
 */
export const writeEnvValue = (file: string, key: string, value: string): void => {
  if (!ENV_SETTINGS.some((s) => s.key === key)) throw new Error(`${key} はコントロール パネルで設定できません`);
  if (/[\r\n]/.test(value)) throw new Error('値に改行は入れられません');
  if (/^['"`]|#/.test(value) && value.includes("'")) throw new Error('# か引用符で始まる値には、単引用符を入れられません');
  const pattern = new RegExp(`^\\s*(export\\s+)?${key}\\s*=`);
  const lines = read(file).split('\n');
  const at = lines.findIndex((l) => pattern.test(l));
  if (at >= 0) lines.splice(at, 1, ...(value ? [lineOf(key, value)] : []));
  else if (value) lines.splice(lines.at(-1) === '' ? -1 : lines.length, 0, lineOf(key, value));
  const text = lines.join('\n');
  fs.writeFileSync(file, text.endsWith('\n') || !text ? text : `${text}\n`, { mode: 0o600 });
  fs.chmodSync(file, 0o600);
};
