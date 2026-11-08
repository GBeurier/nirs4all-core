function exchange = n4aRead(archive, directory, varargin)
%N4AREAD Extract exact member bytes of an owner-neutral .n4a container.
%   EXCHANGE = nirs4all.n4aRead(ARCHIVE, DIRECTORY) extracts every member into
%   the NEW directory DIRECTORY as member-000000.bin, member-000001.bin, ...
%   plus the exact manifest.json, through the native `n4a-read` command.
%   Paths inside the container are never used as operating-system paths; the
%   owner report maps each numbered file to its archive path.
%
%   Name-value options: 'member' (extract one member by its archive path) or
%   'artifact' (extract one artifact's members by ID), mutually exclusive;
%   'expectedSha256' (verify the archive bytes against an independently known
%   SHA-256); 'cli' (NIRS4ALL_CORE_CLI); 'memoryBudgetBytes' (default none: an
%   optional allocation budget in bytes forwarded as the exact decimal text
%   --memory-budget-bytes, see nirs4all.n4aMemoryBudget; this host adds no cap).
%
%   The result holds `directory`, `memberFiles` (numbered files, sorted),
%   `manifestFile`, the owner `report` as decoded by jsondecode and
%   `nativeJson`, the exact report text. No report key is relied upon. Nothing
%   is read, parsed or deserialized in MATLAB/Octave. Readable is not
%   executable: reading needs no Methods runtime or admission.
p = inputParser();
addParameter(p, 'member', ''); addParameter(p, 'artifact', '');
addParameter(p, 'expectedSha256', ''); addParameter(p, 'cli', ''); addParameter(p, 'memoryBudgetBytes', []);
parse(p, varargin{:}); options = p.Results;
budget = nirs4all.n4aMemoryBudget(options.memoryBudgetBytes);
if ~ischar(archive) || ~isrow(archive) || exist(archive, 'file') ~= 2
    error('nirs4all:N4aArgument', 'archive must be the path of an existing file');
end
if ~isempty(options.member) && ~isempty(options.artifact)
    error('nirs4all:N4aArgument', 'member and artifact are mutually exclusive');
end
archive = nirs4all.n4aAbsolutePath(archive);
directory = nirs4all.n4aAbsolutePath(directory);
flags = struct('archive', archive, 'directory', directory);
if ~isempty(options.expectedSha256), flags.expected_archive_sha256 = char(options.expectedSha256); end
if ~isempty(options.member), flags.member = char(options.member); end
if ~isempty(options.artifact), flags.artifact = char(options.artifact); end
if ~isempty(budget), flags.memory_budget_bytes = budget; end
[report, nativeJson, cli] = nirs4all.n4aCall(options.cli, 'n4a-read', flags);
listing = dir(fullfile(directory, 'member-*.bin'));
names = sort({listing.name});
keep = ~cellfun(@isempty, regexp(names, '^member-\d+\.bin$', 'once'));
memberFiles = cellfun(@(n) fullfile(directory, n), names(keep), 'UniformOutput', false);
manifestFile = fullfile(directory, 'manifest.json');
if exist(manifestFile, 'file') ~= 2, manifestFile = ''; end
exchange = struct('operation', 'n4a-read', 'directory', directory, 'memberFiles', {memberFiles}, ...
    'manifestFile', manifestFile, 'report', report, 'nativeJson', nativeJson, 'cli', cli);
end
