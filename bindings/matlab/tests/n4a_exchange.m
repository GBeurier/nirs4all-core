% Owner-neutral .n4a exchange (n4a-write / n4a-read) through the native Core CLI.
%
% Real CLI, no Methods runtime, no model code. The inputs are the files of the
% Core owner fixture bindings/rust/nirs4all/tests/parity/fixtures/n4a_host_opaque_v1
% (layout and report fields: Core author PUBLIC_API.md):
%
%   manifest.json, members.json (member "file" paths are relative to this
%   directory), members/*, expected.json, update/{manifest.json,members.json,members/model.bin}
%
% Environment:
%   NIRS4ALL_CORE_CLI              nirs4all-core-archive that provides n4a-write/read
%   NIRS4ALL_N4A_HOST_FIXTURE_DIR  that fixture directory (copied into a path with spaces)
%   NIRS4ALL_REQUIRE_CORE_N4A=1    qualification mode: any missing prerequisite is an error
%
% Golden containers: expected.json holds expected.container (base) and
% expected.update.container (updated archive), each with byte_length,
% container_sha256 and manifest_sha256, filled from the real owner writer. They
% are REQUIRED: a missing, null or malformed golden fails the test (no bypass).
% Each is compared with the owner write and read reports, the actual archive
% file hash and length, and the SHA-256 of the exact manifest.json read back
% from the archive. Determinism (two writes give the same bytes) is also kept.
% Per-member lengths and SHA-256 come from expected.json.
% It is an Octave script (local functions first, SHA-256 through Octave's
% hash()); MATLAB is not exercised. Nothing is claimed as passed unless
% OCTAVE_N4A_EXCHANGE_PASS is printed.
1;

function bytes = readBytes(path)
fid = fopen(path, 'rb'); bytes = fread(fid, Inf, 'uint8=>uint8'); fclose(fid); bytes = bytes(:);
end
function writeBytes(path, bytes)
fid = fopen(path, 'wb'); fwrite(fid, bytes, 'uint8'); fclose(fid);
end
function digest = fileSha(path)
digest = hash('sha256', char(readBytes(path).'));
end
function writeText(path, text)
fid = fopen(path, 'w'); fprintf(fid, '%s', text); fclose(fid);
end
function id = errorId(thunk)
id = '';
try, thunk(); catch err, id = err.identifier; end
end
function [id, message] = errorInfo(thunk)
id = ''; message = '';
try, thunk(); catch err, id = err.identifier; message = err.message; end
end
function [paths, files, contents] = memberList(listFile)
entries = jsondecode(fileread(listFile));
if isstruct(entries), entries = num2cell(entries); end
count = numel(entries); paths = cell(1, count); files = cell(1, count); contents = cell(1, count);
for i = 1:count
    paths{i} = entries{i}.path;
    files{i} = fullfile(fileparts(listFile), entries{i}.file);   % relative to the list's directory
    assert(exist(files{i}, 'file') == 2);
    contents{i} = readBytes(files{i});
end
end
function checkContainer(label, golden, writeReport, readReport, archive, manifestFile)
% Required golden container: non-null 64-hex hashes and a positive integer length
% that equal the owner reports, the archive file and the exact manifest read back
% from the archive. jsondecode turns null into [], which is refused here.
fields = {'byte_length', 'container_sha256', 'manifest_sha256'};
for i = 1:numel(fields)
    if ~isstruct(golden) || ~isfield(golden, fields{i})
        error('n4a_exchange:Golden', '%s: expected.json lacks %s', label, fields{i});
    end
end
n = golden.byte_length;
if ~(isnumeric(n) && isscalar(n) && isfinite(n) && n > 0 && n == floor(n))
    error('n4a_exchange:Golden', '%s: expected.json byte_length must be a positive integer (golden not filled in)', label);
end
for i = 2:3
    value = golden.(fields{i});
    if ~(ischar(value) && ~isempty(regexp(value, '^[0-9a-f]{64}$', 'once')))
        error('n4a_exchange:Golden', '%s: expected.json %s must be 64 lowercase hex (golden not filled in)', label, fields{i});
    end
end
reports = {writeReport, readReport};
for i = 1:2
    r = reports{i};
    if ~(strcmp(r.archive_sha256, golden.container_sha256) && isnumeric(r.byte_length) && ...
            isscalar(r.byte_length) && r.byte_length == n && strcmp(r.manifest_sha256, golden.manifest_sha256))
        error('n4a_exchange:Golden', '%s: owner %s report differs from the expected.json container', label, r.operation);
    end
end
if ~(strcmp(fileSha(archive), golden.container_sha256) && numel(readBytes(archive)) == n && ...
        strcmp(fileSha(manifestFile), golden.manifest_sha256))
    error('n4a_exchange:Golden', '%s: archive file or its exact manifest.json differs from the expected.json container', label);
end
end
function bytes = contentOf(paths, contents, path)
bytes = contents{strcmp(paths, path)};
end

root = fileparts(fileparts(mfilename('fullpath'))); addpath(root);
cli = getenv('NIRS4ALL_CORE_CLI'); fixtureIn = getenv('NIRS4ALL_N4A_HOST_FIXTURE_DIR');
required = strcmp(getenv('NIRS4ALL_REQUIRE_CORE_N4A'), '1');
work0 = tempname(); mkdir(work0); cleanup0 = onCleanup(@() rmdir(work0, 's')); %#ok<NASGU>
somefile = fullfile(work0, 'some.txt'); writeText(somefile, 'x');

% --- Argument checks that need no native executable --------------------------
assert(strcmp(errorId(@() nirs4all.n4aWrite(struct('a', 1), somefile, fullfile(work0, 'o.n4a'))), 'nirs4all:N4aArgument'));
assert(strcmp(errorId(@() nirs4all.n4aWrite(somefile, somefile, fullfile(work0, 'o.n4a'), 'base', somefile)), 'nirs4all:N4aArgument'));
assert(strcmp(errorId(@() nirs4all.n4aRead(somefile, fullfile(work0, 'd'), 'member', 'a', 'artifact', 'b')), 'nirs4all:N4aArgument'));
assert(strcmp(errorId(@() nirs4all.n4aRead(fullfile(work0, 'missing.n4a'), fullfile(work0, 'd'))), 'nirs4all:N4aArgument'));

% --- Memory budget: exact text, refusals and forwarding (stub CLI, no native work) ---
assert(strcmp(nirs4all.n4aMemoryBudget([]), '') && strcmp(nirs4all.n4aMemoryBudget(''), ''));
assert(strcmp(nirs4all.n4aMemoryBudget(0), '0') && strcmp(nirs4all.n4aMemoryBudget(4096), '4096'));
assert(strcmp(nirs4all.n4aMemoryBudget(9007199254740991), '9007199254740991'));
assert(strcmp(nirs4all.n4aMemoryBudget(intmax('uint64')), '18446744073709551615'));
assert(strcmp(nirs4all.n4aMemoryBudget(intmax('uint64') - 1), '18446744073709551614'));
assert(strcmp(nirs4all.n4aMemoryBudget('18446744073709551615'), '18446744073709551615'));
assert(strcmp(nirs4all.n4aMemoryBudget('007'), '007') && strcmp(nirs4all.n4aMemoryBudget('000'), '000'));
assert(strcmp(nirs4all.n4aMemoryBudget('0018446744073709551615'), '0018446744073709551615'));
invalid = {9007199254740992, 4096.5, -1, NaN, Inf, true, int8(-1), [1 2], {1}, '18446744073709551616', ...
    '99999999999999999999', '0018446744073709551616', '-1', '+1', '1e3', ' 1', '1 ', 'abc', 1 + 2i};
for i = 1:numel(invalid)
    assert(strcmp(errorId(@() nirs4all.n4aMemoryBudget(invalid{i})), 'nirs4all:N4aArgument'), sprintf('invalid budget #%d', i));
end
assert(strcmp(errorId(@() nirs4all.n4aRead(somefile, fullfile(work0, 'd'), 'memoryBudgetBytes', 1.5)), 'nirs4all:N4aArgument'));
assert(strcmp(errorId(@() nirs4all.n4aWrite(somefile, somefile, fullfile(work0, 'o.n4a'), 'memoryBudgetBytes', 'x')), 'nirs4all:N4aArgument'));
if ~ispc
    stub = fullfile(work0, 'stub-cli.sh');
    writeText(stub, ['#!/bin/sh' newline 'out=""; prev=""' newline ...
        'for a in "$@"; do if [ "$prev" = "--output" ]; then out="$a"; fi; prev="$a"; done' newline ...
        'printf ''{"argv":"%s","sample_ids":["a"],"target_names":["t"],"target_units":[null],"values":[[1.5]],"source_id":"s","partition":"p","input_mode":"m","archive_sha256":"x","replay":{},"receipt":{},"training_outcome":{},"training_source_id":"s","source_archive_sha256":"y","readability":{"container_sha256":"z"},"executability":"not_evaluated"}'' "$*" > "$out"' newline]);
    assert(system(['chmod +x ''' stub '''']) == 0);
    flag = '--memory-budget-bytes ';
    got = nirs4all.n4aRead(somefile, fullfile(work0, 'stub read'), 'memoryBudgetBytes', intmax('uint64'), 'cli', stub);
    assert(~isempty(strfind(got.nativeJson, [flag '18446744073709551615'])));
    got = nirs4all.n4aRead(somefile, fullfile(work0, 'stub read 2'), 'memoryBudgetBytes', '4096', 'cli', stub);
    assert(~isempty(strfind(got.nativeJson, [flag '4096'])));
    got = nirs4all.n4aRead(somefile, fullfile(work0, 'stub read 3'), 'memoryBudgetBytes', '', 'cli', stub);
    assert(isempty(strfind(got.nativeJson, '--memory-budget')), 'an empty budget sends no flag');
    got = nirs4all.n4aRead(somefile, fullfile(work0, 'stub read 4'), 'cli', stub);
    assert(isempty(strfind(got.nativeJson, '--memory-budget')), 'a missing budget sends no flag');
    got = nirs4all.n4aWrite(somefile, somefile, fullfile(work0, 'stub write.n4a'), 'memoryBudgetBytes', 7, 'cli', stub);
    assert(~isempty(strfind(got.nativeJson, [flag '7'])));
end

missing = {};
if isempty(cli), missing{end+1} = 'NIRS4ALL_CORE_CLI'; end
if isempty(fixtureIn), missing{end+1} = 'NIRS4ALL_N4A_HOST_FIXTURE_DIR'; end
if ~isempty(missing)
    if required, error('n4a_exchange:Prerequisites', 'Missing required environment: %s', strjoin(missing, ', ')); end
    fprintf(['NOT RUN n4a_exchange: %s not set (depends on the Core fixture n4a_host_opaque_v1); ' ...
        'no native assertion ran\n'], strjoin(missing, ', '));
    return;
end
% A directory with spaces exercises argument quoting; the fixture keeps its
% relative layout inside it, so the owner's own member list is used as is.
work = [tempname() ' n4a exchange']; mkdir(work); cleanup = onCleanup(@() rmdir(work, 's')); %#ok<NASGU>
at = @(varargin) fullfile(work, varargin{:});
fixture = at('host fixture'); mkdir(fixture);
assert(copyfile(fullfile(fixtureIn, '*'), fixture));
ran = {};

% --- CLI-only refusals ---------------------------------------------------------
writeText(at('empty manifest.json'), '{}'); writeText(at('no members.json'), '[]');
assert(strcmp(errorId(@() nirs4all.n4aWrite(at('empty manifest.json'), at('no members.json'), ...
    at('never written.n4a'), 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('never written.n4a'), 'file') ~= 2);
writeText(at('garbage.n4a'), 'not a container');
assert(strcmp(errorId(@() nirs4all.n4aRead(at('garbage.n4a'), at('never extracted'), 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('never extracted'), 'dir') ~= 7);
ran{end+1} = 'cli_refusals=1';

% --- Write the owner fixture, byte for byte ------------------------------------
manifest = fullfile(fixture, 'manifest.json'); members = fullfile(fixture, 'members.json');
[paths, ~, contents] = memberList(members);
expectedPaths = strcat('host/', {'recipe.json', 'data.csv', 'model.bin', 'foreign-model.bin', 'scores.json', 'results.json'});
assert(all(ismember(expectedPaths, paths)));
model = double(contentOf(paths, contents, 'host/model.bin'));   % opaque binary: NUL, 0xFF, CRLF
assert(any(model == 0) && any(model == 255) && any(model(1:end-1) == 13 & model(2:end) == 10));

% The fixture's own expected.json (per-member length and SHA-256, update case,
% refusal names) must agree with the member files and with the report.
expected = jsondecode(fileread(fullfile(fixture, 'expected.json')));
expPaths = arrayfun(@(m) m.path, expected.members, 'UniformOutput', false).';
assert(isequal(expPaths(:).', expectedPaths(:).'));
for i = 1:numel(expected.members)
    m = expected.members(i); want = contentOf(paths, contents, m.path);
    assert(strcmp(m.sha256, hash('sha256', char(want.'))) && m.byte_length == numel(want));
end

aPath = at('host a.n4a');
written = nirs4all.n4aWrite(manifest, members, aPath, 'cli', cli);
shaA = fileSha(aPath); wrep = written.report;
assert(exist(aPath, 'file') == 2 && ischar(written.nativeJson));
assert(strcmp(wrep.operation, 'n4a-write') && strcmp(wrep.archive_sha256, shaA));
assert(wrep.byte_length == numel(readBytes(aPath)) && isempty(wrep.base_archive_sha256));
assert(numel(wrep.members) == numel(paths));
% The report lists the written inventory in manifest order, as in expected.json.
assert(isequal(arrayfun(@(m) m.path, wrep.members, 'UniformOutput', false).', expectedPaths(:).'));
for i = 1:numel(expected.members)
    m = expected.members(i); got = wrep.members(i);
    assert(strcmp(got.path, m.path) && strcmp(got.sha256, m.sha256) && got.byte_length == m.byte_length);
end
% No overwrite, and the same inputs give the same container.
assert(strcmp(errorId(@() nirs4all.n4aWrite(manifest, members, aPath, 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(strcmp(fileSha(aPath), shaA));
nirs4all.n4aWrite(manifest, members, at('host b.n4a'), 'cli', cli);
assert(strcmp(fileSha(at('host b.n4a')), shaA));
% Without a base the member list must cover the manifest; an update list is a
% subset, so the same list used as a fresh write is refused (no output).
assert(strcmp(errorId(@() nirs4all.n4aWrite(manifest, fullfile(fixture, 'update', 'members.json'), ...
    at('host uncovered.n4a'), 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('host uncovered.n4a'), 'file') ~= 2);

% --- Readable, not executable ------------------------------------------------------
info = nirs4all.n4aInspect(aPath, 'cli', cli, 'expectedSha256', shaA);
assert(strcmp(info.executability, 'not_evaluated') && strcmp(info.readability.storage, 'verified'));
assert(strcmp(info.archiveSha256, shaA));
% The host entrypoint belongs to an unknown owner: prediction is refused (the
% runtime arguments are deliberately unusable, so no runtime can have run).
% It must be the OWNER refusal; a missing-runtime error would also be a refusal
% but would show that the runtime was reached before the owner check.
[predictId, predictMessage] = errorInfo(@() nirs4all.n4aPredict(info, 'record', somefile, 'entrypoint', expected.refusals.entrypoint_id, ...
    'methodsLibrary', '/missing/n4a-exchange/libn4m.so', 'methodsLibrarySha256', repmat('0', 1, 64)));
assert(strcmp(predictId, 'nirs4all:N4aRefusal'));
assert(~isempty(strfind(predictMessage, 'owner content for `model` is unresolved')));
assert(isempty(strfind(predictMessage, 'Methods owner runtime is unavailable')));
assert(isempty(strfind(predictMessage, '/missing/n4a-exchange/libn4m.so')));

% --- Read: whole container, then member by member -----------------------------------
whole = nirs4all.n4aRead(aPath, at('read all'), 'expectedSha256', shaA, 'cli', cli);
rrep = whole.report;
assert(strcmp(rrep.operation, 'n4a-read') && strcmp(rrep.archive_sha256, shaA));
assert(strcmp(rrep.selection.kind, 'all') && strcmp(rrep.manifest_file, 'manifest.json'));
assert(numel(rrep.members) == numel(paths) && ~isempty(whole.manifestFile) && numel(whole.memberFiles) == numel(paths));
checkContainer('base container', expected.container, wrep, rrep, aPath, whole.manifestFile);
% Numbering follows the report's members array = manifest inventory order.
for i = 1:numel(rrep.members)
    m = rrep.members(i);
    assert(strcmp(m.path, expectedPaths{i}) && strcmp(m.file, sprintf('member-%06d.bin', i - 1)));
    assert(isequal(readBytes(fullfile(whole.directory, m.file)), contentOf(paths, contents, m.path)));
end
for i = 1:numel(paths)
    one = nirs4all.n4aRead(aPath, at(['read member ' regexprep(paths{i}, '[^A-Za-z0-9]', '_')]), ...
        'member', paths{i}, 'expectedSha256', shaA, 'cli', cli);
    assert(numel(one.memberFiles) == 1);
    [~, name, ext] = fileparts(one.memberFiles{1}); assert(strcmp([name ext], 'member-000000.bin'));
    assert(isequal(readBytes(one.memberFiles{1}), contents{i}));
    assert(strcmp(one.report.selection.kind, 'member'));
    assert(isequal(readBytes(one.manifestFile), readBytes(whole.manifestFile)));
end
% By artifact: "model" owns host/model.bin; the reference artifact "dataset-ref"
% has no embedded member, so it yields none (and still writes manifest.json).
byModel = nirs4all.n4aRead(aPath, at('read artifact model'), 'artifact', 'model', 'cli', cli);
assert(strcmp(byModel.report.selection.kind, 'artifact') && strcmp(byModel.report.selection.value, 'model'));
assert(numel(byModel.report.members) == 1 && strcmp(byModel.report.members(1).path, 'host/model.bin'));
assert(numel(byModel.memberFiles) == 1 && isequal(readBytes(byModel.memberFiles{1}), contentOf(paths, contents, 'host/model.bin')));
byReference = nirs4all.n4aRead(aPath, at('read artifact reference'), 'artifact', 'dataset-ref', 'cli', cli);
assert(isempty(byReference.report.members) && isempty(byReference.memberFiles) && ~isempty(byReference.manifestFile));
% Reads never touch the source; a used directory and unknown names are refused.
assert(strcmp(fileSha(aPath), shaA));
assert(strcmp(errorId(@() nirs4all.n4aRead(aPath, at('read all'), 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(strcmp(errorId(@() nirs4all.n4aRead(aPath, at('read missing'), 'member', expected.refusals.unknown_member, 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('read missing'), 'dir') ~= 7);
assert(strcmp(errorId(@() nirs4all.n4aRead(aPath, at('read unknown artifact'), 'artifact', 'no-such-artifact', 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('read unknown artifact'), 'dir') ~= 7);

% --- Tampered and missing archives ------------------------------------------------------
tampered = at('tampered.n4a'); raw = readBytes(aPath); middle = floor(numel(raw) / 2);
raw(middle) = bitxor(raw(middle), uint8(1)); writeBytes(tampered, raw);
assert(strcmp(errorId(@() nirs4all.n4aRead(tampered, at('read tampered'), 'expectedSha256', shaA, 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('read tampered'), 'dir') ~= 7);
assert(strcmp(errorId(@() nirs4all.n4aRead(tampered, at('read tampered 2'), 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('read tampered 2'), 'dir') ~= 7);
assert(strcmp(errorId(@() nirs4all.n4aRead(aPath, at('read wrong sha'), 'expectedSha256', repmat('0', 1, 64), 'cli', cli)), 'nirs4all:N4aRefusal'));
ran{end+1} = 'roundtrip=1';

% --- Update: immutable base, new container, never overwritten -------------------------
same = nirs4all.n4aWrite(manifest, members, at('host c.n4a'), 'base', aPath, 'baseSha256', shaA, 'cli', cli);
assert(strcmp(same.report.base_archive_sha256, shaA) && strcmp(fileSha(at('host c.n4a')), shaA) && strcmp(fileSha(aPath), shaA));
updManifest = fullfile(fixture, 'update', 'manifest.json'); updMembers = fullfile(fixture, 'update', 'members.json');
[updPaths, updFiles, updContents] = memberList(updMembers);
dPath = at('host d.n4a');
updated = nirs4all.n4aWrite(updManifest, updMembers, dPath, 'base', aPath, 'baseSha256', shaA, 'cli', cli);
shaD = fileSha(dPath); urep = updated.report; replaced = expected.update.replaced(1);
assert(~strcmp(shaD, shaA) && strcmp(fileSha(aPath), shaA));
assert(strcmp(urep.base_archive_sha256, shaA) && strcmp(urep.archive_sha256, shaD));
wholeD = nirs4all.n4aRead(dPath, at('read d all'), 'expectedSha256', shaD, 'cli', cli);
checkContainer('updated container', expected.update.container, urep, wholeD.report, dPath, wholeD.manifestFile);
assert(isequal(updPaths, {replaced.path}));
assert(strcmp(hash('sha256', char(updContents{1}.')), replaced.sha256) && numel(updContents{1}) == replaced.byte_length);
% The written inventory is the update manifest's: replaced + retained, no deleted.
kept = expected.update.retained_from_base; deleted = expected.update.deleted;
if ischar(kept), kept = {kept}; end
if ischar(deleted), deleted = {deleted}; end
reported = arrayfun(@(m) m.path, urep.members, 'UniformOutput', false).';
assert(isequal(sort(reported(:)), sort([{replaced.path}; kept(:)])) && ~any(ismember(deleted, reported)));
assert(isequal(reported(:).', expectedPaths(~ismember(expectedPaths, deleted))));
for i = 1:numel(urep.members)
    m = urep.members(i);
    if strcmp(m.path, replaced.path), want = updContents{1}; else, want = contentOf(paths, contents, m.path); end
    assert(strcmp(m.sha256, hash('sha256', char(want.'))));
end
for i = 1:numel(reported)
    path = reported{i};
    got = nirs4all.n4aRead(dPath, at(['read d ' regexprep(path, '[^A-Za-z0-9]', '_')]), 'member', path, ...
        'expectedSha256', shaD, 'cli', cli);
    % Replaced member: the update bytes. Retained members: the base bytes.
    if strcmp(path, replaced.path), want = updContents{1}; else, want = contentOf(paths, contents, path); end
    assert(isequal(readBytes(got.memberFiles{1}), want));
end
assert(~isequal(updContents{1}, contentOf(paths, contents, replaced.path)));
for i = 1:numel(deleted)
    assert(strcmp(errorId(@() nirs4all.n4aRead(dPath, at('read deleted'), 'member', deleted{i}, 'cli', cli)), 'nirs4all:N4aRefusal'));
    assert(exist(at('read deleted'), 'dir') ~= 7);
    % ...while the immutable base still holds it.
    still = nirs4all.n4aRead(aPath, at('read deleted in base'), 'member', deleted{i}, 'cli', cli);
    assert(isequal(readBytes(still.memberFiles{1}), contentOf(paths, contents, deleted{i})));
end
% Owner refusals during an update: undeclared and duplicate replacements.
writeText(at('undeclared members.json'), jsonencode({struct('path', expected.refusals.undeclared_replacement, 'file', updFiles{1})}));
twice = struct('path', replaced.path, 'file', updFiles{1});
writeText(at('duplicate members.json'), jsonencode({twice, twice}));
assert(strcmp(errorId(@() nirs4all.n4aWrite(updManifest, at('undeclared members.json'), at('host f.n4a'), ...
    'base', aPath, 'baseSha256', shaA, 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(strcmp(errorId(@() nirs4all.n4aWrite(updManifest, at('duplicate members.json'), at('host g.n4a'), ...
    'base', aPath, 'baseSha256', shaA, 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('host f.n4a'), 'file') ~= 2 && exist(at('host g.n4a'), 'file') ~= 2);
% Never overwritten: used destination, the base itself, wrong base hash.
assert(strcmp(errorId(@() nirs4all.n4aWrite(updManifest, updMembers, dPath, 'base', aPath, 'baseSha256', shaA, 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(strcmp(errorId(@() nirs4all.n4aWrite(updManifest, updMembers, aPath, 'base', aPath, 'baseSha256', shaA, 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(strcmp(errorId(@() nirs4all.n4aWrite(updManifest, updMembers, at('host e.n4a'), 'base', aPath, ...
    'baseSha256', repmat('0', 1, 64), 'cli', cli)), 'nirs4all:N4aRefusal'));
assert(exist(at('host e.n4a'), 'file') ~= 2 && strcmp(fileSha(aPath), shaA) && strcmp(fileSha(dPath), shaD));
ran{end+1} = 'update=1';
fprintf('OCTAVE_N4A_EXCHANGE_PASS %s\n', strjoin(ran, ' '));
