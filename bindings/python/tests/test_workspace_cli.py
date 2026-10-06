"""SDK ownership and guaranteed close at the JSON subprocess boundary."""
import json
import os
from pathlib import Path
from unittest.mock import patch

import numpy as np
import pytest
from nirs4all_core.workspace_cli import execute_workspace_command

FIXTURE = os.environ.get("NIRS4ALL_EXPERIMENT_FIXTURE")


@pytest.mark.skipif(not FIXTURE, reason="native model fixture required")
def test_workspace_command_real_sdk_query_predict_and_fitfree(tmp_path):
    from nirs4all.api.session import NativeArchiveSession
    destination = tmp_path / "workspace é space"
    base = {"schema": "nirs4all.workspace-command.v1"}
    opened = execute_workspace_command({**base, "operation": "save", "experiments": [FIXTURE], "destination": str(destination)})
    run_id = opened["runs"][0]["native_run_id"]
    rows = execute_workspace_command({**base, "operation": "query", "path": str(destination), "run_id": run_id})
    assert len(rows) == 5
    data = json.loads((Path(FIXTURE).parent / "independent.json").read_text())
    closed = []
    original = NativeArchiveSession.close
    def close(session):
        original(session)
        closed.append(session.closed)
    from n4m.roles import RolePipeline
    with patch.object(NativeArchiveSession, "close", close), patch.object(RolePipeline, "fit", side_effect=AssertionError("FIT forbidden")):
        result = execute_workspace_command({**base,"operation":"predict","path":str(destination),"run_id":run_id,"X":data["x"],"sample_ids":data["sample_ids"]})
    assert result["sample_ids"] == data["sample_ids"]
    assert np.asarray(result["y_pred"]).shape == (12,1)
    assert closed and all(closed)
    closed.clear()
    with patch.object(NativeArchiveSession, "close", close), patch.object(NativeArchiveSession, "predict", side_effect=ValueError("prediction failed")), pytest.raises(ValueError, match="prediction failed"):
        execute_workspace_command({**base,"operation":"predict","path":str(destination),"run_id":run_id,"X":data["x"],"sample_ids":data["sample_ids"]})
    assert closed and all(closed)
    with pytest.raises(ValueError, match="fields"):
        execute_workspace_command({**base,"operation":"query","path":str(destination),"run_id":run_id,"code":"refused"})
