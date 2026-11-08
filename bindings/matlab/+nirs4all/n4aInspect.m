function model = n4aInspect(archive, varargin)
%N4AINSPECT Read a canonical V1 .n4a container; this does not run anything.
%   MODEL = nirs4all.n4aInspect(ARCHIVE, 'cli', CLI, 'expectedSha256', SHA)
%   returns a reference with the raw archive SHA-256 and the native
%   readability report. Readability is not executability: no Methods runtime
%   is loaded, and `model.executability` is always 'not_evaluated'. Whether an
%   operation can run is decided by the Core/DAG/Methods owners when it is
%   requested. Pre-V1 ZIP archives are refused by Core; native package
%   archives (Package schema 2/3 in a V1 container) use the existing
%   run/predict/load functions.
%   'memoryBudgetBytes' (default none) is an optional allocation budget in
%   bytes, forwarded as the exact decimal text --memory-budget-bytes; see
%   nirs4all.n4aMemoryBudget. Without it no flag is sent and this host adds no
%   cap.
p = inputParser();
addParameter(p, 'cli', ''); addParameter(p, 'expectedSha256', ''); addParameter(p, 'memoryBudgetBytes', []);
parse(p, varargin{:}); options = p.Results;
budget = nirs4all.n4aMemoryBudget(options.memoryBudgetBytes);
archive = nirs4all.n4aAbsolutePath(archive);
if exist(archive, 'file') ~= 2
    error('nirs4all:N4aArgument', 'Archive file not found: %s', archive);
end
flags = struct('archive', archive);
if ~isempty(options.expectedSha256), flags.expected_archive_sha256 = char(options.expectedSha256); end
if ~isempty(budget), flags.memory_budget_bytes = budget; end
[report, nativeJson, cli] = nirs4all.n4aCall(options.cli, 'n4a-inspect', flags);
model = struct('archive', archive, 'archiveSha256', report.readability.container_sha256, ...
    'cli', cli, 'readability', report.readability, 'executability', report.executability, ...
    'nativeJson', nativeJson);
end
