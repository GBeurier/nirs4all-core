function result = dataset(value, coreCli)
% Construct a raw cohort through IO's shared native validator.
if nargin < 2, coreCli = 'nirs4all-core-archive'; end
if isa(value, 'nirs4all.PublicDataset'), result = value; return; end
result = nirs4all.PublicDataset(value, coreCli);
end
