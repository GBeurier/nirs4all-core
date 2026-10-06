function public_multimodal_native(directory, coreCli)
% Synthetic independent cohort, Unicode categories and Python N4MF replay.
addpath(fileparts(fileparts(mfilename('fullpath'))));
dataset = nirs4all.dataset(fullfile(directory,'predict.json'),coreCli);
expected = jsondecode(fileread(fullfile(directory,'expected.json')));
model = nirs4all.MultimodalPredictor.load(fullfile(directory,'predictor.json'),coreCli);
cleanup = onCleanup(@() model.close());
check(model.predict(dataset),expected);
wrong = strrep(dataset.toJSON(),'"wavelength":"nm"','"wavelength":"cm-1"');
if strcmp(wrong,dataset.toJSON())
    wrong = strrep(dataset.toJSON(),'"wavelength": "nm"','"wavelength": "cm-1"');
end
assert(~strcmp(wrong,dataset.toJSON()));
failed = false;
try, model.predict(nirs4all.dataset(wrong,coreCli)); catch, failed = true; end
assert(failed);
recipe = jsondecode(fileread(fullfile(directory,'recipe.json')));
fresh = nirs4all.MultimodalPredictor.fit(recipe,fullfile(directory,'train.json'),coreCli);
closeFresh = onCleanup(@() fresh.close());
check(fresh.predict(dataset),expected);
output = [tempname() '.json']; removeOutput = onCleanup(@() delete(output));
fresh.export(output);
replay = nirs4all.MultimodalPredictor.load(output,coreCli);
closeReplay = onCleanup(@() replay.close());
check(replay.predict(dataset),expected);
fprintf('OCTAVE_MULTIMODAL_PASS Python_state cold_predict raw_fit unicode schema_refusal reload\n');
end

function check(actual,expected)
assert(isequal(actual.sample_ids,expected.sample_ids));
assert(isequal(actual.target_names,expected.target_names));
assert(max(abs(actual.values(:)-expected.values(:))) < 1e-8);
end
