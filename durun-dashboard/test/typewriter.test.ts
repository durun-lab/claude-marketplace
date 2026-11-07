import assert from 'node:assert/strict';
import { test } from 'node:test';
import { revealLength } from '../client/typewriter.ts';

test('revealLength は、短い返答を一定の速さで明かし、最後は全文で止まる', () => {
  const text = 'あ'.repeat(100);
  assert.equal(revealLength(text, 0), 0);
  assert.equal(revealLength(text, 100), 16);
  assert.equal(revealLength(text, 60_000), 100);
});

test('revealLength は、長い返答も決まった時間で書き終える', () => {
  const text = 'a'.repeat(10_000);
  assert.ok(revealLength(text, 1000) > 1000);
  assert.equal(revealLength(text, 2500), 10_000);
});

test('revealLength は、サロゲートペアの途中で切らない', () => {
  const text = 'a😀b';
  for (let ms = 0; ms < 100; ms++) {
    const n = revealLength(text, ms);
    assert.notEqual(n, 2, `${ms}ms で絵文字の途中まで明かした`);
  }
});
