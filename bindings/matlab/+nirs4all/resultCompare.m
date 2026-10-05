function reports = resultCompare(result, variantId, partition)
%RESULTCOMPARE Return stored native score reports for candidate comparison.
if nargin < 2, variantId = ''; end
if nargin < 3, partition = ''; end
if ~isstruct(result) || ~isfield(result, 'score_set')
    error('nirs4all:resultCompare:View', 'result must be a resultView value');
end
source = result.score_set.reports;
if ~isempty(variantId) && ~any(strcmp(result.variant_ids, variantId))
    error('nirs4all:resultCompare:Variant', 'unknown variant_id');
end
keep = false(size(source));
for k = 1:numel(source)
    if iscell(source), report = source{k}; else, report = source(k); end
    if ~isempty(variantId) && (~isfield(report, 'variant_id') || ...
            ~strcmp(report.variant_id, variantId))
        continue;
    end
    if ~isempty(partition) && ~strcmp(report.partition, partition)
        continue;
    end
    keep(k) = true;
end
reports = source(keep);
for k = 1:numel(reports)
    if iscell(reports), report = reports{k}; else, report = reports(k); end
    report.is_winner = isfield(report, 'variant_id') && ...
        strcmp(report.variant_id, result.winner_variant_id);
    if iscell(reports), reports{k} = report; else, reports(k).is_winner = report.is_winner; end
end
end
