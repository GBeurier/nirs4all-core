% SPDX-License-Identifier: CeCILL-2.1 OR AGPL-3.0-or-later
% Public document retention only; no native discovery, fitting or prediction.
addpath(fullfile(fileparts(mfilename('fullpath')), '..'));

snv = struct('class', 'n4m.SNV', 'params', struct('with_std', false, 'ddof', 3));
model = struct('model', struct('class', 'sklearn.cross_decomposition.PLSRegression', ...
    'params', struct('scale', false, 'n_components', 5)));
recipe = struct('steps', {{struct('_comment', 'Keep this source position.'), snv, model}}, ...
    'target', struct('name', 'response', 'grain', 'sample'), ...
    'selection', struct('metric', 'rmse'), ...
    'provenance', struct('source', 'example.v1.2'));
definition = nirs4all.loadPipelineDefinition(recipe);
assert(isequal(definition.source, recipe));
assert(numel(definition.pipeline) == 2);
recipe.target.name = 'changed caller value';
assert(strcmp(definition.source.target.name, 'response'));

direct = {struct('_comment', 'Keep me.'), struct('class', 'n4m.SNV')};
definition = nirs4all.loadPipelineDefinition(direct);
assert(isequal(definition.source, direct));
assert(numel(definition.pipeline) == 1);
singleArray = '[{"class":"n4m.SNV"}]';
definition = nirs4all.loadPipelineDefinition(singleArray);
assert(strcmp(definition.source, singleArray));
assert(numel(definition.pipeline) == 1);

% A structured comment must disappear only from the display copy.
commented = struct('class', 'n4m.SNV', '_comment', 'Keep the source comment.');
definition = nirs4all.loadPipelineDefinition({commented});
assert(isequal(definition.source, {commented}));
assert(~isfield(definition.pipeline{1}, '_comment'));

jsonText = ['{"pipeline":[{"class":"n4m.SNV"}],"search":{"seed":17}}' newline];
yamlText = ['# Retain the original YAML comment.' newline ...
    'pipeline:' newline '  - class: n4m.SNV' newline ...
    'target:' newline '  name: response' newline '  grain: sample' newline];
texts = {jsonText, yamlText};
suffixes = {'.json', '.yaml'};
directory = tempname(); mkdir(directory);
cleanup = onCleanup(@() rmdir(directory, 's'));
for idx = 1:numel(texts)
    definition = nirs4all.loadPipelineDefinition(texts{idx});
    assert(strcmp(definition.source, texts{idx}));
    path = fullfile(directory, ['recipe' suffixes{idx}]);
    file = fopen(path, 'w');
    assert(file >= 0);
    fprintf(file, '%s', texts{idx}); fclose(file);
    fromFile = nirs4all.loadPipelineDefinition(path);
    assert(strcmp(fromFile.source, texts{idx}));
    assert(isequal(fromFile.pipeline, definition.pipeline));
end

fprintf('pipeline_document: original documents retained; no fits.\n');
