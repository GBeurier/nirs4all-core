function result = resumeTuning(previous, data, trials, varargin)
%RESUMETUNING Continue the native study without replaying completed fits.
c=previous.config;
if isfield(previous,'cli') && ~any(strcmpi(varargin(1:2:end),'cli'))
    varargin = [varargin {'cli',previous.cli}];
end
result=nirs4all.tune(data,'trials',trials,'seed',c.seed,'sampler',c.sampler, ...
 'metric',c.metric,'sourceId',c.source_id,'checkpoint',previous.archive,varargin{:});
end
