import {
  readPortableArchiveV2,
  writeNativePredictorPackage,
  type PortableArchiveV2Payloads,
} from '../../src/index.js';

export async function transportPortableArchive(
  outcome: Uint8Array,
  packageJson: Uint8Array,
): Promise<PortableArchiveV2Payloads> {
  const bytes: Uint8Array = await writeNativePredictorPackage('archive:consumer', outcome, packageJson);
  const inventory = await readPortableArchiveV2(bytes, { entrypoint: 'predict-external' });
  const packageBytes: Uint8Array = inventory.package;
  const evidence: Uint8Array | null = inventory.trainingOutcome;
  const version: 2 | 3 = inventory.view.package_schema_version;
  const digest: string = inventory.archiveSha256;
  void packageBytes;
  void evidence;
  void version;
  void digest;
  return inventory;
}

// @ts-expect-error Archives are bytes, not JSON text.
void readPortableArchiveV2('{}');
// @ts-expect-error Native package JSON must be exact bytes, not text.
void writeNativePredictorPackage('archive:consumer', '{}', '{}');
