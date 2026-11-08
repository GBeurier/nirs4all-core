function [report, nativeJson, cli] = n4aCall(cli, operation, flags)
%N4ACALL Run one canonical V1 .n4a command of the native Core CLI.
%   FLAGS is a struct of character values; underscores in field names become
%   hyphens (source_id -> --source-id). The decoded JSON report and its exact
%   native text are returned. Refusals keep the native message and are raised as
%   nirs4all:N4aRefusal; there is no fallback to another archive route.
try
    [report, nativeJson, cli] = nirs4all.workflowCli(cli, operation, '', flags);
catch err
    if strcmp(err.identifier, 'nirs4all:NativeCli'), rethrow(err); end
    detail = err.message;
    if ~isempty(strfind(detail, ['unknown archive command: ' operation]))
        error('nirs4all:N4aUnavailable', ...
            ['The native CLI does not provide %s. Use a nirs4all-core-archive build ' ...
             'that includes the canonical n4a-* commands.'], operation);
    end
    error('nirs4all:N4aRefusal', '%s: %s', operation, detail);
end
end
