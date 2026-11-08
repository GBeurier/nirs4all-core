function exchange = n4aWrite(manifestPath, membersPath, destination, varargin)
%N4AWRITE Seal or update an owner-neutral .n4a container from exact files.
%   EXCHANGE = nirs4all.n4aWrite(MANIFESTPATH, MEMBERSPATH, DESTINATION) sends
%   the manifest JSON file and the member list file (a JSON array of
%   {path, file} objects, each `file` being a binary file with the exact member
%   bytes) to the native `n4a-write` command. Nothing is encoded, parsed or
%   deserialized here: bytes and array shapes reach Core unchanged. Data,
%   recipes, results, scores and opaque host models are all just members.
%
%   UPDATE: EXCHANGE = nirs4all.n4aWrite(..., 'base', BASE, 'baseSha256', SHA)
%   updates an immutable base. BASE is verified against the independently known
%   SHA-256, is never modified, and the result is a new container at
%   DESTINATION. Members that MEMBERSPATH does not list come from BASE.
%   DESTINATION is never overwritten.
%
%   The result holds `path`, the owner `report` as decoded by jsondecode and
%   `nativeJson`, the exact report text. No report key is relied upon.
%
%   Readable is not executable: writing needs no Methods runtime or admission,
%   and an opaque model stays a byte member that is never loaded or run here.
%   Name-value option 'cli' selects the executable (NIRS4ALL_CORE_CLI).
%   'memoryBudgetBytes' (default none) is an optional allocation budget in
%   bytes, forwarded as the exact decimal text --memory-budget-bytes; see
%   nirs4all.n4aMemoryBudget. Without it no flag is sent and this host adds no
%   cap.
p = inputParser();
addParameter(p, 'base', ''); addParameter(p, 'baseSha256', ''); addParameter(p, 'cli', '');
addParameter(p, 'memoryBudgetBytes', []);
parse(p, varargin{:}); options = p.Results;
budget = nirs4all.n4aMemoryBudget(options.memoryBudgetBytes);
manifestPath = existingFile(manifestPath, 'manifestPath');
membersPath = existingFile(membersPath, 'membersPath');
destination = nirs4all.n4aAbsolutePath(destination);
flags = struct('manifest', manifestPath, 'members', membersPath, 'destination', destination);
if xor(isempty(options.base), isempty(options.baseSha256))
    error('nirs4all:N4aArgument', 'base and baseSha256 must be given together');
end
if ~isempty(options.base)
    flags.archive = existingFile(options.base, 'base');
    flags.expected_archive_sha256 = char(options.baseSha256);
end
if ~isempty(budget), flags.memory_budget_bytes = budget; end
[report, nativeJson, cli] = nirs4all.n4aCall(options.cli, 'n4a-write', flags);
exchange = struct('operation', 'n4a-write', 'path', destination, 'report', report, ...
    'nativeJson', nativeJson, 'cli', cli);
end

function path = existingFile(path, name)
if ~ischar(path) || ~isrow(path) || exist(path, 'file') ~= 2
    error('nirs4all:N4aArgument', ['%s must be the path of an existing file; structs and ' ...
        'in-memory values are refused because re-encoding would change the bytes'], name);
end
path = nirs4all.n4aAbsolutePath(path);
end
