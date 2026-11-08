// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { locate } from '../src/evidence.mjs';

/** A real root with `data/file.txt` (5 bytes), an empty directory `dir`, and a sibling tree next to the root. */
async function tree(t) {
  const base = await realpath(await mkdtemp(path.join(os.tmpdir(), 'spikes-locate-')));
  t.after(() => rm(base, { recursive: true, force: true }));
  const root = path.join(base, 'root');
  await mkdir(path.join(root, 'data'), { recursive: true });
  await mkdir(path.join(root, 'dir'));
  await writeFile(path.join(root, 'data', 'file.txt'), 'hello');
  await mkdir(path.join(base, 'root-evil'));
  await writeFile(path.join(base, 'root-evil', 'secret.txt'), 'secret');
  await writeFile(path.join(base, 'outside.txt'), 'outside');
  return { base, root };
}

test('locate: a file inside the root gives its real path and size', async (t) => {
  const { root } = await tree(t);
  assert.deepEqual(await locate(root, 'data/file.txt'), { state: 'ok', abs: path.join(root, 'data', 'file.txt'), size: 5 });
});

test('locate: a path with .. that stays inside the root is accepted', async (t) => {
  const { root } = await tree(t);
  const loc = await locate(root, 'dir/../data/file.txt');
  assert.equal(loc.state, 'ok');
});

for (const [name, rel] of [
  ['an empty string', ''],
  ['a number', 5],
  ['null', null],
  ['undefined', undefined],
  ['the root itself', '.'],
  ['a parent path', '..'],
  ['a file in the parent directory', '../outside.txt'],
  ['a sibling directory whose name starts like the root', '../root-evil/secret.txt'],
]) {
  test(`locate: ${name} is outside the root`, async (t) => {
    const { root } = await tree(t);
    assert.deepEqual(await locate(root, rel), { state: 'outside' });
  });
}

test('locate: an absolute path is outside the root even when the file is inside it', async (t) => {
  const { root } = await tree(t);
  assert.deepEqual(await locate(root, path.join(root, 'data', 'file.txt')), { state: 'outside' });
});

test('locate: a symbolic link that leaves the root is outside', async (t) => {
  const { base, root } = await tree(t);
  await symlink(path.join(base, 'outside.txt'), path.join(root, 'link.txt'));
  assert.deepEqual(await locate(root, 'link.txt'), { state: 'outside' });
});

test('locate: a symbolic link to a sibling directory that starts like the root is outside', async (t) => {
  const { base, root } = await tree(t);
  await symlink(path.join(base, 'root-evil'), path.join(root, 'evil'));
  assert.deepEqual(await locate(root, 'evil/secret.txt'), { state: 'outside' });
});

test('locate: a symbolic link that stays inside the root is followed', async (t) => {
  const { root } = await tree(t);
  await symlink(path.join(root, 'data', 'file.txt'), path.join(root, 'alias.txt'));
  assert.deepEqual(await locate(root, 'alias.txt'), { state: 'ok', abs: path.join(root, 'data', 'file.txt'), size: 5 });
});

test('locate: a missing file is missing', async (t) => {
  const { root } = await tree(t);
  assert.deepEqual(await locate(root, 'data/none.txt'), { state: 'missing' });
});

test('locate: a directory is missing', async (t) => {
  const { root } = await tree(t);
  assert.deepEqual(await locate(root, 'dir'), { state: 'missing' });
});

test('locate: an error other than "not found" is thrown, not hidden', async (t) => {
  const { root } = await tree(t);
  // A file used as a directory gives ENOTDIR.
  await assert.rejects(locate(root, 'data/file.txt/inner'), { code: 'ENOTDIR' });
});
