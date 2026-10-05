function public_multimodal_content(directory, coreCli)
expected = jsondecode(fileread(fullfile(directory,'content-expected.json')));
for name = {'predict.json','logical-host-dataset.json','reordered-host-dataset.json'}
    ds = nirs4all.dataset(fullfile(directory,name{1}),coreCli);
    actual = jsondecode(ds.invoke(ds.toJSON(),'dataset-content'));
    assert(strcmp(actual.fingerprint,expected.fingerprint));
    assert(strcmp(actual.canonical_content_utf8,expected.canonical_content_utf8));
end
fid=fopen(fullfile(directory,'octave-content-evidence.json'),'w');
cleanup=onCleanup(@() fclose(fid)); %#ok<NASGU>
fprintf(fid,'%s',jsonencode(struct('fingerprint',expected.fingerprint)));
disp('PASS Octave logical raw-content bytes equal Python/R/JS');
end
