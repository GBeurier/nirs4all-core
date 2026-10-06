function result = workspaceCli(python, operation, fields)
%WORKSPACECLI Invoke the SDK workspace bridge with argv, without a shell.
if nargin < 1 || isempty(python), python = getenv('NIRS4ALL_WORKSPACE_PYTHON'); end
if isempty(python), python = 'python3'; end
if ~(ischar(python) || (isstring(python) && isscalar(python)))
    error('nirs4all:workspace', 'python must be one executable path');
end
request = fields; request.schema = 'nirs4all.workspace-command.v1'; request.operation = operation;
input = [tempname() '.json']; output = [tempname() '.json']; logPath = [tempname() '.log'];
cleanup = onCleanup(@() cleanupFiles({input, output, logPath})); %#ok<NASGU>
fid = fopen(input, 'wb');
if fid < 0, error('nirs4all:workspace', 'Cannot create workspace request'); end
fileCleanup = onCleanup(@() fclose(fid));
fwrite(fid, unicode2native(jsonencode(request), 'UTF-8'), 'uint8');
clear fileCleanup;
args = {'-m', 'nirs4all_core.workspace_cli', '--input', input, '--output', output};
if exist('OCTAVE_VERSION', 'builtin') && exist('popen2', 'builtin')
    [writer, reader, pid] = popen2(char(python), args);
    if pid < 0, error('nirs4all:workspace', 'Cannot start Python workspace bridge'); end
    fclose(writer);
    processCleanup = onCleanup(@() fclose(reader)); %#ok<NASGU>
    started = tic();
    while true
        fread(reader, 65536, 'uint8'); fclear(reader);
        [waited, status] = waitpid(pid, WNOHANG());
        if waited ~= 0, break; end
        if toc(started) > 120
            kill(pid, 15); waitpid(pid, 0);
            error('nirs4all:workspace', 'Python workspace bridge timed out');
        end
        pause(0.01);
    end
    if waited ~= pid, error('nirs4all:workspace', 'Cannot wait for workspace bridge'); end
    status = bitshift(status, -8);
elseif usejava('jvm')
    argv = javaObject('java.util.ArrayList'); argv.add(char(python));
    for index = 1:numel(args), argv.add(args{index}); end
    builder = javaObject('java.lang.ProcessBuilder', argv);
    builder.directory(javaObject('java.io.File', pwd));
    builder.redirectErrorStream(true); builder.redirectOutput(javaObject('java.io.File', logPath));
    process = builder.start();
    processCleanup = onCleanup(@() closeProcess(process)); %#ok<NASGU>
    status = process.waitFor();
else
    error('nirs4all:workspace', 'Workspace access requires Octave popen2 or a MATLAB JVM');
end
if exist(output, 'file') ~= 2
    error('nirs4all:workspace', 'Python/Core plus the full SDK are required by the workspace bridge');
end
response = jsondecode(fileread(output));
if ~response.ok, error('nirs4all:workspace', '%s', response.error); end
if status ~= 0, error('nirs4all:workspace', 'Inconsistent workspace bridge status'); end
result = response.result;
end
function cleanupFiles(paths)
for index = 1:numel(paths), if exist(paths{index}, 'file') == 2, delete(paths{index}); end, end
end
function closeProcess(process)
try, process.getOutputStream().close(); catch, end
try, process.getInputStream().close(); catch, end
try, process.getErrorStream().close(); catch, end
try, process.destroy(); catch, end
end
