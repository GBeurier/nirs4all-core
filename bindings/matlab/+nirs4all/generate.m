function variants = generate(choices, varargin)
%GENERATE Native constrained Cartesian/zip variants or seeded random subset.
parser=inputParser; addParameter(parser,'strategy','cartesian'); addParameter(parser,'constraints',struct());
addParameter(parser,'count',[]); addParameter(parser,'seed',0); addParameter(parser,'maxVariants',10000);
addParameter(parser,'cli',''); parse(parser,varargin{:}); o=parser.Results;
if ~isnumeric(o.seed) || ~isscalar(o.seed) || ~isfinite(o.seed) || o.seed<0 || o.seed>flintmax-1 || o.seed~=fix(o.seed)
 error('nirs4all:GenerationSeed','seed must be a nonnegative exactly representable integer');
end
record=struct('choices',choices,'strategy',o.strategy,'constraints',o.constraints,'count',o.count,'seed',o.seed,'max_variants',o.maxVariants);
if isempty(o.count), record=rmfield(record,'count'); end
reply=nirs4all.workflowCli(o.cli,'generate',record,struct()); variants=reply.variants;
if ~isfield(reply,'seed_decimals') || numel(reply.seed_decimals)~=numel(variants)
 error('nirs4all:GenerationSeed','Native generator requires exact decimal seed transport');
end
for i=1:numel(variants), variants(i).seed=reply.seed_decimals{i}; end
end
