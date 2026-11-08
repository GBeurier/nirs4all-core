% SPDX-License-Identifier: CeCILL-2.1 OR AGPL-3.0-or-later
% Public-facade guard check only: no dataset, kernel, controller or numerical fit.
addpath(fullfile(fileparts(mfilename('fullpath')), '..'));

snv = struct('class', 'n4m.SNV');
model = struct('model', struct('class', 'n4m.PLSRegression', ...
    'params', struct('n_components', 1)));
expectedMessage = 'Unsupported portable KennardStone parameter ''n_train'': training-count splits are not supported by this execution profile.';

suppliedCounts = {struct('n_train', 2), struct('n_train', 2, 'test_size', 0.5), ...
    struct('n_train', 0), struct('n_train', false)};
for idx = 1:numel(suppliedCounts)
    ks = struct('class', 'n4m.KennardStone', 'params', suppliedCounts{idx});
    source = struct('pipeline', {{ks, snv, model}});
    refused = false;
    try
        nirs4all.runPortablePipeline(source, struct());
    catch err
        assert(strcmp(err.identifier, 'nirs4all:UnsupportedOperator'));
        assert(strcmp(err.message, expectedMessage));
        refused = true;
    end
    assert(refused, 'A supplied count must refuse before Methods discovery or dataset coercion.');
end

% Valid omissions must pass parsing, but the empty dataset can never reach a fit.
% [] is the local empty/null count representation; explicit fractions are unchanged.
unsetCounts = {struct(), struct('n_train', []), struct('n_train', '', 'test_size', 0.375)};
for idx = 1:numel(unsetCounts)
    ks = struct('class', 'n4m.KennardStone', 'params', unsetCounts{idx});
    source = struct('pipeline', {{ks, snv, model}});
    stoppedWithoutFit = false;
    try
        nirs4all.runPortablePipeline(source, struct());
    catch err
        assert(any(strcmp(err.identifier, {'nirs4all:MissingMethods', 'nirs4all:InvalidDataset'})));
        stoppedWithoutFit = true;
    end
    assert(stoppedWithoutFit, 'Omission control must stop at missing Methods or the empty dataset.');
end

fprintf('portable_count_refusal: supplied counts refused and omissions accepted through parsing; no fits.\n');
