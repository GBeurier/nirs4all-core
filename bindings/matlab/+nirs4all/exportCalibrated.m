function path = exportCalibrated(model, path)
%EXPORTCALIBRATED Copy the native model and calibrator together.
if exist(path, 'file'), error('nirs4all:DestinationExists', 'Destination exists'); end
[ok, detail] = copyfile(model.archive, path);
if ~ok, error('nirs4all:ArchiveCopy', '%s', detail); end
end
