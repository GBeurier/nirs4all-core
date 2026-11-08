import assert from 'node:assert/strict';
import test from 'node:test';

import { loadIo } from '../src/index.js';

// Core's public `loadIo()` hands out the IO owner's WASM module; these tests only translate owner behavior.
// They need the current `@nirs4all/io-wasm` Node peer and fail explicitly when it is absent (no skip, no fake module).

const encode = (text) => new TextEncoder().encode(text);

// 20 rows x 700 numeric columns, no header line (the IO owner's own regression literal): column 0 varies, the rest are 0.2.
function headerlessTwentyBySevenHundred() {
  let text = '';
  for (let row = 0; row < 20; row += 1) {
    text += `0.${String(465041 + row * 10000).padStart(7, '0')}`;
    for (let column = 1; column < 700; column += 1) text += ';0.2';
    text += '\n';
  }
  return text;
}

test('headerless data.csv infers no header through loadIo and keeps all twenty rows', async () => {
  const io = await loadIo();
  assert.equal(typeof io.inferFiles, 'function');
  assert.equal(typeof io.assembleDataset, 'function');
  const files = [{ name: 'data.csv', bytes: encode(headerlessTwentyBySevenHundred()) }];

  // Without a user choice the owner defaults to a header and flags the ambiguity; it never guesses `false`.
  const ambiguous = io.inferFiles(files, null);
  assert.equal(ambiguous.params['data.csv'].header_ambiguous, true);
  assert.ok(
    ambiguous.warnings.some((warning) => warning.includes('header_ambiguous') && warning.includes('data.csv')),
  );
  assert.notEqual(ambiguous.resolved_spec.sources[0].params.has_header, false);

  // The explicit choice is declared in the FIRST inference call; the plan is never patched afterwards.
  const plan = io.inferFiles(files, { file_params: { 'data.csv': { has_header: false } } });
  assert.notEqual(plan.params['data.csv'].header_ambiguous, true);
  assert.equal(plan.resolved_spec.sources[0].params.has_header, false);
  // A declared choice suppresses the ambiguity warning (the owner warns only for undeclared headers).
  assert.equal(plan.warnings.filter((warning) => warning.includes('header_ambiguous')).length, 0);

  const full = io.assembleDataset(files, [], JSON.stringify(plan.resolved_spec));
  const x = full.blocks.train.x[0];
  assert.equal(x.n_rows, 20, 'the first line is a data row, not a header');
  assert.equal(x.n_cols, 699);
  assert.equal(full.blocks.train.feature_headers[0].length, 699);
  assert.ok(x.data instanceof Float64Array);
  assert.equal(x.data.length, 20 * 699);
  assert.equal(x.data[0], 0.0465041);
  assert.ok(x.data.slice(1, 699).every((value) => value === 0.2));
  assert.equal(x.data[19 * 699], 0.0655041);
  assert.ok(x.data.slice(19 * 699 + 1).every((value) => value === 0.2));
  const targets = Buffer.from(new Float64Array(20).fill(0.2).buffer).toString('hex');
  assert.equal(full.blocks.train.y.columns[0].block.data_bytes_hex, targets);

  // Control: forcing a header line consumes the first data row, which is the 19-row defect shape.
  const explicit = structuredClone(plan.resolved_spec);
  explicit.sources[0].params.has_header = true;
  explicit.sources[0].role = 'features';
  delete explicit.sources[0].columns;
  const consumed = io.assembleDataset(files, [], JSON.stringify(explicit));
  assert.equal(consumed.blocks.train.x[0].n_rows, 19);
  assert.equal(consumed.blocks.train.x[0].data[0], 0.0475041);
});

const LITERAL = 'Sample_ID;set;1100;target\nS_missing;train;;1.0\nS_finite;train;2.0;2.0\nS_test;test;10.0;3.0\n';

// The IO owner's literal probe spec; only the NA configuration varies.
const probeSpec = (na) => ({
  schema_version: 1,
  name: 'statistical-na-mean-heldout-probe',
  task_type: 'regression',
  sample_index: { by: 'id', key: 'Sample_ID' },
  params: { delimiter: ';', decimal: '.', has_header: true, na },
  sources: [{
    id: 'x', role: 'mixed', input: 'literal.csv', key: 'Sample_ID',
    params: { header_unit: 'nm' },
    columns: { features: ['1100'], targets: ['target'], metadata: ['set'] },
  }],
  partitions: {
    by: 'column', column: 'set', train_values: ['train'],
    test_values: ['test'], unknown_policy: 'error',
  },
});

test('mean and median NA fills are refused by the owner even when the policy is inactive', async () => {
  const io = await loadIo();
  const files = [{ name: 'literal.csv', bytes: encode(LITERAL) }];
  const refusal = /na\.fill\.method: .* is refused: .*fitted preprocessing/;
  const perSample = /within each sample/;
  for (const method of ['mean', 'median']) {
    // `ignore` makes the fill inactive; the owner still refuses the statistical method.
    for (const policy of ['replace', 'ignore']) {
      for (const sourceOverride of [false, true]) {
        const na = { policy, fill: { method, per_column: true } };
        const value = probeSpec(sourceOverride ? { policy: 'ignore' } : na);
        if (sourceOverride) value.sources[0].params.na = na;
        const text = JSON.stringify(value);
        for (const expected of [refusal, perSample]) {
          assert.throws(() => io.validate(text), expected);
          assert.throws(() => io.assembleDataset(files, [], text), expected);
        }
      }
    }
  }

  // Control: the identical structure with only the method changed is a valid owner specification.
  io.validate(JSON.stringify(probeSpec({ policy: 'replace', fill: { method: 'forward_fill' } })));

  // The supported `ignore` control still assembles the same file with its exact values and masks.
  const ignored = JSON.stringify(probeSpec({ policy: 'ignore' }));
  io.validate(ignored);
  const full = io.assembleDataset(files, [], ignored);
  const ids = (block) => block.metadata.columns.find((column) => column.name === 'Sample_ID').values;
  assert.deepEqual(ids(full.blocks.train), ['S_missing', 'S_finite']);
  assert.deepEqual(ids(full.blocks.test), ['S_test']);
  const train = full.blocks.train.x[0];
  assert.equal(train.data_bytes_hex, '00000000000000000000000000000040');
  assert.deepEqual(train.validity, [false, true]);
  assert.ok(Number.isNaN(train.data[0]));
  assert.equal(train.data[1], 2);
  assert.equal(full.blocks.train.y.columns[0].block.data_bytes_hex, '000000000000f03f0000000000000040');
  assert.equal(full.blocks.test.x[0].data[0], 10);
});
