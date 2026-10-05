function public_multimodal(directory, coreCli)
train = nirs4all.dataset(fullfile(directory, 'train.json'), coreCli);
new = nirs4all.dataset(fullfile(directory, 'predict.json'), coreCli);
expected = jsondecode(fileread(fullfile(directory, 'expected.json')));
replay = nirs4all.MultimodalPredictor.load(fullfile(directory, 'predictor.json'), coreCli);
cleanup = onCleanup(@() replay.close());
check(replay.predict(new), expected);
clear cleanup;
replay = nirs4all.MultimodalPredictor.load(fullfile(directory, 'predictor-wasm.json'), coreCli);
cleanup = onCleanup(@() replay.close());
check(replay.predict(new), expected);
% Keep the exact canonical JSON; decoding and reencoding null changes host arrays.
wrong = strrep(new.toJSON(), '"wavelength": "nm"', '"wavelength": "cm-1"');
if strcmp(wrong, new.toJSON()), wrong = strrep(new.toJSON(), '"wavelength":"nm"', '"wavelength":"cm-1"'); end
failed = false;
try, replay.predict(nirs4all.dataset(wrong, coreCli)); catch, failed = true; end
assert(failed);
clear cleanup;
recipe = jsondecode(fileread(fullfile(directory, 'recipe.json')));
fresh = nirs4all.MultimodalPredictor.fit(recipe, train, coreCli);
cleanup = onCleanup(@() fresh.close());
check(fresh.predict(new), expected);
fresh.export(fullfile(directory, 'predictor-octave.json'));
fresh.close(); fresh.close(); clear cleanup;
disp('PASS Octave actual U07 fit, Python/WASM cold raw N4MF replay, identity, schema errors');
end
function check(result, expected)
assert(isequal(result.sample_ids, expected.sample_ids));
assert(isequal(result.target_names, expected.target_names));
assert(max(abs(result.values(:) - expected.values(:))) < 1e-8);
end
