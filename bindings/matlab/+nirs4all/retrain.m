function fresh = retrain(workflow, dataset, varargin)
%RETRAIN Run a new native CV campaign with the saved user configuration.
c=workflow.config;
fresh=nirs4all.run(dataset,'sourceId',c.sourceId,'components',c.components, ...
    'preprocessing',c.preprocessing,varargin{:});
end
