import { inspectN4aContainer, type N4aInspectionPort, type N4aInspectionDocument } from '../../src/index.js';

const port: N4aInspectionPort = {
  owner: 'example', contentType: 'example.document', version: 1,
  inspect: (_bytes, artifact) => ({ artifactId: artifact.id, kind: 'document', title: artifact.id,
    status: 'not_evaluated', detail: 'Report only', fields: [], tables: [] }),
};
async function inspectionConsumer(file: Blob): Promise<N4aInspectionDocument> {
  const session = await inspectN4aContainer(file, { previewers: [port] });
  try { const report = session.report(); await session.preview('example', { maxRows: 32 }); return report; }
  finally { session.close(); }
}
void inspectionConsumer;
