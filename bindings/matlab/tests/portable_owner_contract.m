% SPDX-License-Identifier: CeCILL-2.1 OR AGPL-3.0-or-later
% Actual owner admission only. Missing Core commands fail; no fits or skips.
addpath(fullfile(fileparts(mfilename('fullpath')), '..'));
admitted = {};
for withStd = [true false]
    for scale = [true false]
        snv = struct('class', 'nirs4all.operators.transforms.SNV', ...
            'params', struct('with_std', withStd));
        model = struct('model', struct('class', 'sklearn.cross_decomposition.PLSRegression', ...
            'params', struct('n_components', 2, 'scale', scale)));
        request = struct('schema_version', 1, 'definition', struct('pipeline', {{snv, model}}));
        try
            plan = nirs4all.workflowCli('', 'portable-pipeline-plan', jsonencode(request), struct());
        catch err
            allowed = (~withStd && ~isempty(strfind(err.message, '.params.with_std'))) || ...
                (~scale && ~isempty(strfind(err.message, '.params.scale')));
            assert(allowed, err.message);
            continue
        end
        if isstruct(plan.recipe), steps = num2cell(plan.recipe); else, steps = plan.recipe; end
        assert(strcmp(plan.profile, 'portable_linear_regression.v1'));
        assert(strcmp(plan.mode, 'training'));
        assert(steps{1}.params.with_std == withStd);
        assert(steps{end}.params.scale_x == scale && steps{end}.params.scale_y == scale);
        assert(strcmp(steps{end}.params.solver, 'simpls'));
        admitted{end + 1} = jsonencode(plan.recipe); %#ok<AGROW>
    end
end
assert(numel(admitted) == numel(unique(admitted)));

% A complete YAML root control must reach owner refusal rather than vanish
% from the loader's cleaned display. The empty dataset cannot be fitted.
source = ['pipeline:' newline '  - model:' newline ...
    '      class: n4m.PLSRegression' newline ...
    'target:' newline '  name: response' newline '  grain: sample' newline];
refused = false;
try
    nirs4all.runPortablePipeline(source, struct());
catch err
    assert(any(strcmp(err.identifier, {'nirs4all:InvalidPipeline', 'nirs4all:UnsupportedOperator'})));
    assert(~isempty(strfind(err.message, '$.target')));
    refused = true;
end
assert(refused, 'A supplied root control must be admitted or refused by the owner.');

% Production adapters may marshal arrays but cannot retain scientific paths.
implementation = fileread(fullfile(fileparts(mfilename('fullpath')), '..', '+nirs4all', 'runPortablePipeline.m'));
for token = {'n4m.pls_fit', 'n4m.kennard_stone_split', 'n4m.snv_transform', ...
        'n4m.savgol_transform', 'predictFromCoefficients', 'sqrt(mean', 'plan.nComponents'}
    assert(isempty(strfind(implementation, token{1})), token{1});
end
fprintf('portable_owner_contract: native no-fit admission checked; no fits.\n');
