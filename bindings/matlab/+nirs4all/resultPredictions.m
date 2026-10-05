function rows = resultPredictions(result, variantId, partition, foldId)
%RESULTPREDICTIONS Extract stored native rows with sample identity preserved.
if nargin < 2, variantId = ''; end
if nargin < 3, partition = ''; end
if nargin < 4, foldId = ''; end
if ~isstruct(result) || ~isfield(result, 'predictions')
    error('nirs4all:resultPredictions:View', 'result must be a resultView value');
end
source = result.predictions;
if ~isempty(variantId) && ~any(strcmp(result.variant_ids, variantId))
    error('nirs4all:resultPredictions:Variant', 'unknown variant_id');
end
keep = false(size(source));
for k = 1:numel(source)
    row = source(k);
    if (~isempty(variantId) && ~strcmp(row.variant_id, variantId)) || ...
            (~isempty(partition) && ~strcmp(row.partition, partition)) || ...
            (~isempty(foldId) && ~strcmp(row.fold_id, foldId))
        continue;
    end
    keep(k) = true;
end
rows = source(keep);
end
