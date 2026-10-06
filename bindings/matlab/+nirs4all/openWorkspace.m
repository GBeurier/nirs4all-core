function value = openWorkspace(path, python)
%OPENWORKSPACE Open a validated modern SDK workspace through Python/Core.
if nargin < 2, python = getenv('NIRS4ALL_WORKSPACE_PYTHON'); end
value = nirs4all.Workspace(path, python);
end
