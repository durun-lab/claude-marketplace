// ツールを使わない補助の問い合わせを、claude -p の代わりに Claude API で動かす。API のクレジットで払い、サブスクリプションの使用制限を減らさない。
// claude -p は環境変数の ANTHROPIC_API_KEY があるとそれで動くので、キーは別の名前で読み、claude -p とセッションに渡さない。
import Anthropic from '@anthropic-ai/sdk';
import type { TokenTotals } from '../shared/types.ts';

export const API_KEY_ENV = 'DURUN_DASHBOARD_ANTHROPIC_API_KEY';

export type ApiModel = 'haiku' | 'sonnet' | 'opus';

// 長い文章を書き直すと出力が長くなるので、ストリームで受けて HTTP の時間切れを避ける。
const MAX_TOKENS = 64_000;

type Params = Pick<Anthropic.Beta.MessageCreateParams, 'model' | 'thinking' | 'output_config' | 'betas' | 'fallbacks'>;

// claude -p で動かしていたときと同じく、haiku と sonnet は思考を外して速く返させ、opus は思考を外さずに effort を medium にする。
// sonnet と opus は、安全の判定で断られたときに別のモデルで答え直させる。
const FALLBACK = { betas: ['server-side-fallback-2026-07-01'], fallbacks: 'default' } satisfies Partial<Params>;
const PARAMS: Record<ApiModel, Params> = {
  haiku: { model: 'claude-haiku-5-5', thinking: { type: 'disabled' }, output_config: { effort: 'low' } },
  sonnet: { model: 'claude-sonnet-5-5', thinking: { type: 'between_tools' }, ...FALLBACK },
  opus: { model: 'claude-opus-5-5', output_config: { effort: 'medium' }, ...FALLBACK },
};

let client: Anthropic | undefined;

/** API のキーがあれば、問い合わせに使うクライアントを返す。 */
export const apiClient = (): Anthropic | undefined => {
  const apiKey = process.env[API_KEY_ENV];
  if (!apiKey) return undefined;
  client ??= new Anthropic({ apiKey });
  return client;
};

export type ApiAnswer = { text: string; stopReason: string | null; usage: TokenTotals };

/** system と prompt を渡し、JSON Schema に沿った答えの文を返す。timeout を過ぎたら、SDK が中止して例外を投げる。 */
export const askApi = async (api: Anthropic, model: ApiModel, system: string, prompt: string, schema: object, timeout: number): Promise<ApiAnswer> => {
  const p = PARAMS[model];
  const message = await api.beta.messages
    .stream(
      {
        ...p,
        max_tokens: MAX_TOKENS,
        system,
        messages: [{ role: 'user', content: prompt }],
        output_config: { ...p.output_config, format: { type: 'json_schema', schema: schema as Record<string, unknown> } },
      },
      { timeout },
    )
    .finalMessage();
  const text = message.content.flatMap((b) => (b.type === 'text' ? [b.text] : [])).join('');
  const u = message.usage;
  return {
    text,
    stopReason: message.stop_reason,
    usage: { input: u.input_tokens, output: u.output_tokens, cacheRead: u.cache_read_input_tokens ?? 0, cacheCreation: u.cache_creation_input_tokens ?? 0 },
  };
};
