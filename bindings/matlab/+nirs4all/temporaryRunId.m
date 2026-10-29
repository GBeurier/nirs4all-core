function id = temporaryRunId(prefix)
%TEMPORARYRUNID Keep the unique temporary token independent of TMPDIR length.
% DAG identifiers are limited to 128 ASCII bytes. Directory names are transport
% details and must not become part of a run or its derived outcome identifiers.
[~, token, extension] = fileparts(tempname());
token = regexprep([token extension], '[^A-Za-z0-9_.:-]', '_');
id = [prefix token];
end
