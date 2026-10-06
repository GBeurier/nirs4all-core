function fresh = retrain(workflow, dataset, varargin)
%RETRAIN Run a new native CV campaign with the saved user configuration.
c=workflow.config;
if isfield(workflow,'cli') && ~any(strcmpi(varargin(1:2:end),'cli'))
    varargin = [varargin {'cli',workflow.cli}];
end
fresh=nirs4all.run(dataset,'sourceId',c.sourceId,'components',c.components, ...
    'preprocessing',c.preprocessing,varargin{:});
end
