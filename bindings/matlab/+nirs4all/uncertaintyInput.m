function payload = uncertaintyInput(X, sampleIds, sourceId, extras, cli)
%UNCERTAINTYINPUT Preserve owner IO JSON or explicit positional row arrays.
% Positional input carries no declaration of axes, units or physical groups.
structured = isa(X, 'nirs4all.PublicDataset') || isstruct(X) || ischar(X);
if structured
    if ~isempty(sampleIds), error('nirs4all:Uncertainty','Structured uncertainty uses Dataset sample IDs; omit sampleIds'); end
    if ~isa(X,'nirs4all.PublicDataset'), X = nirs4all.dataset(X, cli); end
    fields = {['"dataset":' X.toJSON()]};
    if ~isempty(sourceId), fields{end+1} = ['"source_id":' jsonencode(sourceId)]; end
else
    if isempty(sampleIds), error('nirs4all:Uncertainty','Positional uncertainty requires sampleIds'); end
    if ~isempty(sourceId), error('nirs4all:Uncertainty','sourceId requires a structured Dataset'); end
    if ~isnumeric(X) || ndims(X)~=2 || any(~isfinite(X(:)))
        error('nirs4all:Uncertainty','Finite numeric matrix required');
    end
    rows = arrayfun(@(i) num2cell(double(X(i,:))), 1:size(X,1), 'UniformOutput', false);
    fields = {['"x":' jsonencode(rows)], ['"sample_ids":' jsonencode(cellstr(sampleIds))]};
end
names = fieldnames(extras);
for k=1:numel(names)
    fields{end+1} = [jsonencode(names{k}) ':' jsonencode(extras.(names{k}))]; %#ok<AGROW>
end
payload = ['{' strjoin(fields, ',') '}'];
end
