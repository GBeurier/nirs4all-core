classdef WorkspaceSession < handle
    % Native SDK predictor reopened per call; each command closes resources.
    properties (SetAccess = private)
        closed = false
    end
    properties (Access = private)
        workspace
        runId
    end
    methods
        function obj = WorkspaceSession(workspace, runId)
            workspace.requireOpen(); obj.workspace = workspace; obj.runId = char(runId);
        end
        function value = predict(obj, X, sampleIds)
            if obj.closed, error('nirs4all:workspace', 'Workspace session is closed'); end
            obj.workspace.requireOpen();
            if ~isnumeric(X) || ~ismatrix(X) || size(X,1) ~= numel(sampleIds)
                error('nirs4all:workspace', 'X must be a numeric matrix with explicit sample IDs');
            end
            rows = arrayfun(@(index) num2cell(X(index,:)), 1:size(X,1), 'UniformOutput',false);
            if isstring(sampleIds), sampleIds = cellstr(sampleIds); end
            request = struct('path',obj.workspace.path,'run_id',obj.runId,'X',{rows},'sample_ids',{reshape(sampleIds,1,[])});
            value = nirs4all.workspaceCli(obj.workspace.python, 'predict', request);
        end
        function close(obj), obj.closed = true; end
        function delete(obj), obj.close(); end
    end
end
