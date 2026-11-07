// 手元の環境変数に API のキーがあっても、テストは claude -p の偽物で問い合わせを受ける。本物の API を呼ばず、結果を環境に左右させないため。
import { API_KEY_ENV } from '../server/claudeApi.ts';

delete process.env[API_KEY_ENV];
