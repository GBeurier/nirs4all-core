"""Generate synthetic fixtures through the released native workflow contracts.

No SDK, private measurements or Python numerical implementation is involved.
The deterministic dense dataset is maintained with the MATLAB/Octave tests.
"""
import argparse
import copy
import json
import os
import subprocess
import tempfile
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cli", default=os.environ.get("NIRS4ALL_CORE_CLI", "nirs4all-core-archive"))
    parser.add_argument("--methods-library", default=os.environ.get("N4M_LIBRARY_PATH"))
    args = parser.parse_args()
    if not args.methods_library:
        parser.error("--methods-library or N4M_LIBRARY_PATH is required")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    fixture = Path(__file__).resolve().parents[2] / "bindings/matlab/tests/fixtures/workflow_dense.json"
    train = json.loads(fixture.read_text())

    def invoke(operation, record=None, **flags):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "output.json"
            command = [args.cli, operation, "--output", str(output)]
            if record is not None:
                request = Path(tmp) / "input.json"
                request.write_text(json.dumps(record, allow_nan=False))
                command += ["--input", str(request)]
            for key, value in flags.items():
                command += ["--" + key.replace("_", "-"), str(value)]
            subprocess.run(command, check=True)
            return json.loads(output.read_text())

    invoke("workflow-run", train, source_id="spectra", components="[1,2]",
           methods_library=args.methods_library, archive=root / "shared-model.n4a",
           run_id="run:qualification:fixture", results_directory=root / "results")

    def cohort(prefix):
        data = copy.deepcopy(train)
        ids = [prefix + value for value in train["dataset"]["sample_ids"]]
        data["origin_ids"] = ids
        data["dataset"]["sample_ids"] = ids
        data["dataset"]["sources"][0]["sample_ids"] = ids
        return data

    calibration, current = cohort("cal."), cohort("test.")
    invoke("conformal-calibrate", calibration, archive=root / "shared-model.n4a",
           destination=root / "shared-calibrated.n4a", source_id="spectra", coverages="[0.8,0.9]",
           small_sample_policy='"error"', methods_library=args.methods_library,
           run_id="run:qualification:calibrate")
    x = current["dataset"]["sources"][0]["array"]["values"]
    y, ids = current["dataset"]["y"]["values"], current["dataset"]["sample_ids"]
    flags = dict(archive=root / "shared-calibrated.n4a", methods_library=args.methods_library)
    prediction = invoke("conformal-predict", {"x": x, "sample_ids": ids},
                        run_id="run:qualification:prediction", **flags)
    report = invoke("robustness", {"x": x, "sample_ids": ids, "truth": [[v] for v in y],
        "scenarios": [{"id": "observed", "kind": "observed", "severity": 0, "seed": 0},
                      {"id": "gaussian", "kind": "spectral_noise", "severity": 0.01, "seed": 1}]},
        run_id="run:qualification:robustness", **flags)
    current["dataset"]["y"] = None
    current["dataset"]["partitions"] = {"dtype": "<U7", "shape": [len(ids)], "values": ["predict"] * len(ids)}
    records = {"train": train, "dataset-calibration": calibration, "dataset-predict": current,
               "independent": {"x": x, "y": y, "sample_ids": ids},
               "shared-prediction": prediction, "shared-robustness": report}
    for name, record in records.items():
        (root / f"{name}.json").write_text(json.dumps(record, allow_nan=False))
    invoke("experiment-save", input=root / "results", destination=root / "experiment",
           archive=root / "shared-model.n4a")
    print(f"Generated shared native workflow fixtures: {root}")


if __name__ == "__main__":
    main()
