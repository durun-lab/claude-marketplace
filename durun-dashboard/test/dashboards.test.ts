import assert from 'node:assert/strict';
import { test } from 'node:test';
import { dashboardsOf, putDashboard } from '../server/dashboards.ts';
import { FIRST_DASHBOARDS } from '../shared/types.ts';

test('putDashboard は、まだ一覧が無ければ最初のダッシュボードに配置を保存し、名前を省いたら今の名前を残す', () => {
  const saved = putDashboard(undefined, { id: '', layout: [{ id: 'inbox' }] });
  assert.deepEqual(saved, { list: FIRST_DASHBOARDS.list, layouts: { '': [{ id: 'inbox' }] } });
});

test('putDashboard は、名前のある新しい id を足し、既にある id は名前だけを変えて配置を残す', () => {
  const added = putDashboard(undefined, { id: '1', name: '作業用', layout: [] });
  assert.deepEqual(added.list, [...FIRST_DASHBOARDS.list, { id: '1', name: '作業用' }]);
  const renamed = putDashboard(added, { id: '1', name: 'モバイル' });
  assert.deepEqual(renamed.list.at(-1), { id: '1', name: 'モバイル' });
  assert.deepEqual(renamed.layouts, { 1: [] });
});

test('putDashboard は、一覧に無い id に名前の無い配置だけが届いても、ダッシュボードを作らない', () => {
  const saved = putDashboard(FIRST_DASHBOARDS, { id: 'gone', layout: [] });
  assert.equal(saved, FIRST_DASHBOARDS);
});

test('dashboardsOf は、ブラウザから引き継ぐ一覧と配置の形を確かめ、余計な項目を落とす', () => {
  assert.deepEqual(dashboardsOf({ list: [{ id: '', name: 'A', extra: 1 }], layouts: { '': [] } }), { list: [{ id: '', name: 'A' }], layouts: { '': [] } });
  assert.equal(dashboardsOf({ list: [], layouts: {} }), undefined);
  assert.equal(dashboardsOf({ list: [{ id: '' }], layouts: {} }), undefined);
  assert.equal(dashboardsOf({ list: [{ id: '', name: 'A' }], layouts: { '': {} } }), undefined);
});
