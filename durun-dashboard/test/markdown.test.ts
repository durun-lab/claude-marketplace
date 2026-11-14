import assert from 'node:assert/strict';
import { test } from 'node:test';
import { imgOf } from '../shared/html.ts';
import { lexer } from 'marked';
import { type DetailsBlock, firstHeading, groupDetails } from '../shared/markdown.ts';

test('firstHeading は、最初の見出しを返し、コードブロックの中の # とフロントマターは見出しとみなさない', () => {
  assert.equal(firstHeading('前書き\n\n## 設計メモ\n\n# 後の見出し\n'), '設計メモ');
  assert.equal(firstHeading('```sh\n# コメント\n```\n\n# 本当の見出し\n'), '本当の見出し');
  assert.equal(firstHeading('---\ntitle: x\n---\n\n# 題名\n'), '題名');
  assert.equal(firstHeading('下線の見出し\n===\n'), '下線の見出し');
  assert.equal(firstHeading('見出しの無い文\n'), undefined);
});

test('imgOf は、<img> を 1 つだけ書いた HTML から src と alt を読み、ほかの HTML は読まない', () => {
  const src = 'https://github.com/user-attachments/assets/00000000-0000-0000-0000-000000000000';
  assert.deepEqual(imgOf(`<img width="1707" height="995" alt="image" src="${src}" />\n`), { src, alt: 'image' });
  assert.deepEqual(imgOf("<img data-src='a.png' src='b.png'>"), { src: 'b.png', alt: '' });
  assert.equal(imgOf(`<a href="${src}"><img src="${src}"></a>`), undefined);
  assert.equal(imgOf('<img src="a.png"><img src="b.png">'), undefined);
  assert.equal(imgOf('<img alt="src の無い画像">'), undefined);
});

const isDetails = (t: unknown): t is DetailsBlock => (t as DetailsBlock | undefined)?.type === 'details';

test('groupDetails は、<details> から </details> までを、題と中の Markdown を持つ 1 つのブロックにまとめる', () => {
  const blocks = (text: string) => groupDetails(lexer(text), lexer).filter((t) => t.type !== 'space');
  const [before, details, after] = blocks('前\n\n<details>\n<summary>詳細な<b>ログ</b></summary>\n\n- 中身\n\n```\ncode\n```\n\n</details>\n\n後\n');
  assert.equal(before?.type, 'paragraph');
  assert.equal(after?.type, 'paragraph');
  assert.ok(isDetails(details));
  assert.equal(details.summary, '詳細なログ');
  assert.equal(details.open, false);
  assert.deepEqual(details.tokens.filter((t) => t.type !== 'space').map((t) => t.type), ['list', 'code']);
  const [inline] = blocks('<details open><summary>題</summary>本文</details>\n');
  assert.ok(isDetails(inline));
  assert.equal(inline.open, true);
  assert.deepEqual(inline.tokens.map((t) => t.type), ['paragraph']);
  const [nested, tail] = blocks('<details>\n\n<details>\n\n中\n\n</details>\n\n外\n\n</details>\n\n後\n');
  assert.ok(isDetails(nested));
  assert.equal(nested.summary, '詳細');
  assert.equal(tail?.type, 'paragraph');
});
