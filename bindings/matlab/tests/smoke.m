addpath(fullfile(fileparts(mfilename('fullpath')), '..'));

items = nirs4all.upstreams();
assert(numel(items) == 6);
assert(strcmp(items(1).key, 'dag_ml'));
method_item = nirs4all.requireUpstream('methods');
assert(strcmp(method_item.key, 'methods'));

try
    nirs4all.requireUpstream('missing');
    error('nirs4all:testFailed', 'missing upstream should fail');
catch err
    assert(strcmp(err.identifier, 'nirs4all:UnknownUpstream'));
end

disp('nirs4all MATLAB/Octave smoke passed');
