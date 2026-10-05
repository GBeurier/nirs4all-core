function public_multimodal_archive(directory, coreCli, dagCli, adapterDirectory, octave)
options = struct('core_cli',coreCli,'dag_cli',dagCli,'adapter_directory',adapterDirectory,'octave',octave);
expected = jsondecode(fileread(fullfile(directory, 'expected.json')));
result = nirs4all.predictMultimodalArchive(fullfile(directory,'u07-native.n4a'), fullfile(directory,'predict.json'), options);
assert(isequal(result.sample_ids,cellstr(expected.sample_ids)));
assert(isequal(result.target_names,cellstr(expected.target_names)));
assert(max(abs(result.values(:)-expected.values(:))) < 1e-8);
assert(~result.training_performed);
assert(isequal(cellfun(@(event) event.operation,result.audit,'UniformOutput',false),{'hydrate','PREDICT','dispose','release'}));
if exist(fullfile(directory,'logical-host-dataset.json'),'file') == 2
    logicalResult = nirs4all.predictMultimodalArchive(fullfile(directory,'u07-native.n4a'),fullfile(directory,'logical-host-dataset.json'),options);
    assert(isequal(logicalResult.sample_ids,cellstr(expected.sample_ids)));
    assert(max(abs(logicalResult.values(:)-expected.values(:)))<1e-8);
    assert(isequal(cellfun(@(event) event.operation,logicalResult.audit,'UniformOutput',false),{'hydrate','PREDICT','dispose','release'}));
    fid=fopen(fullfile(directory,'octave-logical-archive-evidence.json'),'w');fprintf(fid,'%s',jsonencode(logicalResult));fclose(fid);
end
record = fileread(fullfile(directory,'predict.json'));
wrong = strrep(record,'"wavelength": "nm"','"wavelength": "cm-1"');
if strcmp(record,wrong), wrong = strrep(record,'"wavelength":"nm"','"wavelength":"cm-1"'); end
failed = false;
try
    nirs4all.predictMultimodalArchive(fullfile(directory,'u07-native.n4a'),wrong,options);
catch exception
    failed = ~isempty(strfind(exception.message,'schema differs'));
end
assert(failed);
fid=fopen(fullfile(directory,'octave-core-archive-evidence.json'),'w'); fprintf(fid,'%s',jsonencode(result)); fclose(fid);
disp('PASS Octave unchanged native .n4a archive, independent raw cohort, no training, schema failure');
end
