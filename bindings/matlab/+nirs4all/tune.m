function workflow = tune(data, varargin)
%TUNE Native OOF HPO on PLS components 1..3 with a total trial budget.
parser=inputParser; addParameter(parser,'trials',8); addParameter(parser,'seed',91);
addParameter(parser,'sampler','random'); addParameter(parser,'metric','rmse'); addParameter(parser,'sourceId','spectra');
addParameter(parser,'methodsLibrary',getenv('N4M_LIB_PATH')); addParameter(parser,'archive',[tempname() '.n4a']);
addParameter(parser,'checkpoint',''); addParameter(parser,'runId',['run:' strrep(tempname(),filesep,'_')]); addParameter(parser,'cli','');
parse(parser,varargin{:}); o=parser.Results;
if isempty(o.cli) && isa(data, 'nirs4all.PublicDataset'), o.cli = data.coreCli; end
if ~isnumeric(o.seed) || ~isscalar(o.seed) || ~isfinite(o.seed) || o.seed<0 || o.seed>flintmax-1 || o.seed~=fix(o.seed)
 error('nirs4all:TuningSeed','seed must be a nonnegative exact integer in 0..2^53-1');
end
if ~isnumeric(o.trials) || ~isscalar(o.trials) || ~isfinite(o.trials) || o.trials<1 || o.trials>256 || o.trials~=fix(o.trials)
 error('nirs4all:TuningBudget','trials must be a total budget between 1 and 256');
end
if isempty(o.methodsLibrary), o.methodsLibrary=getenv('N4M_LIBRARY_PATH'); end
flags=struct('trials',sprintf('%.0f',o.trials),'seed',sprintf('%.0f',o.seed),'sampler',o.sampler,'metric',o.metric, ...
 'source_id',o.sourceId,'methods_library',o.methodsLibrary,'archive',o.archive,'run_id',o.runId);
if ~isempty(o.checkpoint), flags.checkpoint_archive=o.checkpoint; end
[outcome, ~, cli]=nirs4all.workflowCli(o.cli,'tuning-run',data,flags);
config=struct('source_id',o.sourceId,'trials',o.trials,'seed',o.seed,'sampler',o.sampler,'metric',o.metric);
workflow=struct('archive',outcome.model_archive,'outcome',outcome,'config',config,'cli',cli);
end
