import {
  readPortableArchiveV2,
  writePortableArchiveV2,
  type PortableArchiveV2Payloads,
} from '../../src/index.js';

export async function transportPortableArchive(
  manifest: Record<string, unknown>,
  members: Record<string, Uint8Array>,
): Promise<PortableArchiveV2Payloads> {
  const bytes: Uint8Array = await writePortableArchiveV2(manifest, members);
  const inventory = await readPortableArchiveV2(bytes);
  const packageBytes: Uint8Array = inventory.members['dagml/portable_predictor_package.json'];
  const digest: string = inventory.archiveSha256;
  void packageBytes;
  void digest;
  return inventory;
}

// @ts-expect-error Archives are bytes, not JSON text.
void readPortableArchiveV2('{}');
// @ts-expect-error Opaque members must retain exact byte payloads.
void writePortableArchiveV2({}, { 'dagml/package.json': '{}' });
