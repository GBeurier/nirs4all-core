classdef Workspace < handle
    % SDK SQLite/Parquet workspace. Requires Python/Core and the full SDK.
    properties (SetAccess = private)
        path
        python
        closed = false
    end
    properties (Access = private)
        children = {}
    end
    methods
        function obj = Workspace(path, python)
            if nargin < 2, python = getenv('NIRS4ALL_WORKSPACE_PYTHON'); end
            if isempty(python), python = 'python3'; end
            obj.path = char(path); obj.python = char(python);
            opened = nirs4all.workspaceCli(obj.python, 'open', struct('path', obj.path));
            obj.path = opened.path;
        end
        function value = runs(obj)
            obj.requireOpen(); value = nirs4all.workspaceCli(obj.python, 'open', struct('path',obj.path)); value = value.runs;
        end
        function value = predictions(obj, runId)
            obj.requireOpen(); value = nirs4all.workspaceCli(obj.python, 'query', struct('path',obj.path,'run_id',char(runId)));
        end
        function value = session(obj, runId)
            obj.requireOpen(); runs = obj.runs();
            if ~any(strcmp({runs.native_run_id}, char(runId))), error('nirs4all:workspace', 'Unknown workspace run'); end
            value = nirs4all.WorkspaceSession(obj, runId); obj.children{end+1} = value;
        end
        function value = export(obj, destination)
            obj.requireOpen(); value = nirs4all.workspaceCli(obj.python, 'export', struct('path',obj.path,'destination',char(destination))); value = value.archive;
        end
        function close(obj)
            if ~obj.closed
                for index = 1:numel(obj.children), obj.children{index}.close(); end
                obj.children = {}; obj.closed = true;
            end
        end
        function delete(obj), obj.close(); end
        function requireOpen(obj)
            if obj.closed, error('nirs4all:workspace', 'Workspace is closed'); end
        end
    end
    methods (Static)
        function value = importSnapshot(archive, destination, python)
            if nargin < 3, python = getenv('NIRS4ALL_WORKSPACE_PYTHON'); end
            nirs4all.workspaceCli(python, 'import', struct('archive',char(archive),'destination',char(destination)));
            value = nirs4all.Workspace(destination, python);
        end
    end
end
