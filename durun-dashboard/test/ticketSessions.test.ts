import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ticketSessionsText } from '../server/ticketSessions.ts';
import { fromAgent, promptKind, type Session } from '../shared/types.ts';

const pr = 'https://github.com/o/r/pull/12';
const jira = 'https://example.atlassian.net/browse/WEB-7';
const session = (id: string, extra: Partial<Session>): Session => ({ id, name: id, repo: 'r', status: 'idle', archived: false, prs: [], jira: [], ...extra }) as Session;

test('チケットの関連セッションは、自分を除き、タスクにしたものを先に、アーカイブ済みを後に並べる', () => {
  const sessions = [
    session('self', { prs: [{ url: pr, number: 12 } as Session['prs'][number]] }),
    session('old', { archived: true, prs: [{ url: pr, number: 12 } as Session['prs'][number]] }),
    session('plain', { status: 'busy', branch: 'fix/x', prs: [{ url: pr, number: 12 } as Session['prs'][number]] }),
    session('owner', { prs: [{ url: pr, number: 12 } as Session['prs'][number]], agentTasks: [{ id: '1', subject: 'PR', status: 'in_progress', ticket: pr }] }),
    session('issue', { jira: [{ key: 'WEB-7', url: jira }] }),
  ];
  assert.deepEqual(ticketSessionsText(sessions, '#12', 'self').split('\n').map((l) => l.match(/session_id: (\S+)、/)?.[1]), ['owner', 'plain', 'old']);
  assert.match(ticketSessionsText(sessions, pr, 'self'), /名前: plain、状態: 作業中、リポジトリ: r、ブランチ: fix\/x/);
  assert.match(ticketSessionsText(sessions, pr, 'self'), /このチケットのタスク: 作業中/);
  assert.match(ticketSessionsText(sessions, 'web-7', 'self'), /session_id: issue/);
  assert.match(ticketSessionsText(sessions, jira, 'self'), /session_id: issue/);
  assert.equal(ticketSessionsText(sessions, 'WEB-8', 'self'), 'WEB-8 をバッジにしているほかのセッションはありません。');
});

test('チャットの指示を、人間・エージェント・ワークフローの印で分ける', () => {
  assert.equal(promptKind('直してください'), 'prompt');
  assert.equal(promptKind(fromAgent('親', 'p', '続けて')), 'agent-prompt');
  assert.equal(promptKind('[durun-dashboard ワークフロー]「改善要望の対応」 改善要望が 1 件'), 'workflow-prompt');
  assert.equal(promptKind('/compact'), 'compact');
});
