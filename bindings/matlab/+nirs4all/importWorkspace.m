function value = importWorkspace(archive, destination, python)
%IMPORTWORKSPACE Import an immutable SDK snapshot without replacing a path.
if nargin < 3, python = getenv('NIRS4ALL_WORKSPACE_PYTHON'); end
value = nirs4all.Workspace.importSnapshot(archive, destination, python);
end
