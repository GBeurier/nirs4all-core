function result = dataset(value, coreCli)
% Construct a raw cohort through IO's shared native validator.
if isa(value, 'nirs4all.PublicDataset')
    if nargin < 2 || isempty(coreCli) || strcmp(coreCli,value.coreCli), result = value; return; end
    value = value.toJSON();
end
if nargin < 2 || isempty(coreCli), coreCli = getenv('NIRS4ALL_CORE_CLI'); end
if isempty(coreCli), coreCli = 'nirs4all-core-archive'; end
result = nirs4all.PublicDataset(value, coreCli);
end
