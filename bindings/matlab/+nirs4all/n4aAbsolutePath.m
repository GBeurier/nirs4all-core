function path = n4aAbsolutePath(path)
%N4AABSOLUTEPATH Make a file path absolute without requiring that it exists.
%   The native CLI receives paths exactly as given, so relative paths are
%   resolved against the current directory before the call.
path = char(path);
if isempty(path) || ~isrow(path) || any(path == char(0)) || any(path == newline)
    error('nirs4all:N4aArgument', 'A nonempty character row path is required');
end
isAbsolute = path(1) == '/' || (ispc && numel(path) > 1 && path(2) == ':') || strncmp(path, '\\', 2);
if ~isAbsolute, path = fullfile(pwd(), path); end
end
