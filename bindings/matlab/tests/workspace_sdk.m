function workspace_sdk()
fixture = getenv('NIRS4ALL_WORKSPACE_FIXTURE');
assert(~isempty(fixture));
workspace = nirs4all.openWorkspace(fixture);
cleanup = onCleanup(@() workspace.close()); %#ok<NASGU>
runs = workspace.runs(); run = runs(1).native_run_id;
rows = workspace.predictions(run); assert(numel(rows) == 5);
data = jsondecode(fileread(getenv('NIRS4ALL_WORKSPACE_PREDICT_DATA')));
oracle = jsondecode(fileread(getenv('NIRS4ALL_WORKSPACE_PREDICT_ORACLE')));
session = workspace.session(run);
predicted = session.predict(data.x, data.sample_ids);
assert(isequal(predicted.sample_ids, data.sample_ids));
assert(max(abs(predicted.y_pred(:) - oracle.y_pred(:))) < 1e-12);
archive = [tempname() ' é snapshot.n4w']; destination = [tempname() ' é imported'];
fileCleanup = onCleanup(@() removePaths(archive,destination)); %#ok<NASGU>
workspace.export(archive);
imported = nirs4all.importWorkspace(archive, destination);
assert(numel(imported.predictions(run)) == numel(rows)); imported.close();
workspace.close(); assert(session.closed);
failed=false;try,workspace.predictions(run);catch,failed=true;end;assert(failed);
failed=false;try,session.predict(data.x,data.sample_ids);catch,failed=true;end;assert(failed);
disp('PASS SDK workspace Octave query/session/native parity/export/import/Unicode/lifecycle');
end
function removePaths(archive,destination)
if exist(archive,'file')==2,delete(archive);end
if exist(destination,'dir')==7,rmdir(destination,'s');end
end
