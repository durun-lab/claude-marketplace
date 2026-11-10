// 書きかけのメッセージは、画面を移っても残るようセッションごとにブラウザへ保存する。
const draftKey = (sessionId: string): string => `message-draft:${sessionId}`;

export const loadDraft = (sessionId: string): string => {
  try {
    return localStorage.getItem(draftKey(sessionId)) ?? '';
  } catch {
    return '';
  }
};

export const saveDraft = (sessionId: string, text: string): void => {
  try {
    if (text) localStorage.setItem(draftKey(sessionId), text);
    else localStorage.removeItem(draftKey(sessionId));
  } catch {
    // 保存できなくても、画面上の書きかけはそのまま使える。
  }
};

/** 送信欄に書きかけの文が残っているか。空白だけなら書いていないとみなす。 */
export const hasDraft = (sessionId: string): boolean => loadDraft(sessionId).trim() !== '';
