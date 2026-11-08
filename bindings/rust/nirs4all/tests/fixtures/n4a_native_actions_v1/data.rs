//! Additive literal data; no native/provider or artifact fixture is mocked.
use serde_json::{json, Value};

pub fn record(prefix: &str, rows: usize, training: bool, labelled: bool) -> Value {
    let ids: Vec<_> = (0..rows)
        .map(|i| format!("{prefix}-{:02}", rows - i))
        .collect();
    let x: Vec<Vec<f64>> = (0..rows)
        .map(|r| {
            (0..7)
                .map(|c| {
                    ((r + 1) as f64 * (c + 1) as f64 * 0.13).sin()
                        + r as f64 * 0.11
                        + c as f64 * 0.2
                })
                .collect()
        })
        .collect();
    let y: Vec<_> = x
        .iter()
        .enumerate()
        .map(|(r, row)| {
            vec![
                2.0 * row[0] - row[2] + r as f64 * 0.13,
                -31.0 + 7.0 * row[1] + row[4] - r as f64 * 0.27,
            ]
        })
        .collect();
    let mut result = json!({
        "schema":"nirs4all.dataset.v1", "schema_version":1,
        "origin_ids":ids, "fold_ids":vec![Value::Null; rows],
        "dataset":{
            "schema":"nirs4all.multimodal-dataset", "schema_version":1,
            "name":format!("f10-{prefix}"), "sample_ids":ids,
            "sources":[{"name":"spectra", "sample_ids":ids,
                "representation_id":"signal_1d", "axes":["sample","wavelength"],
                "feature_names":null, "axis_units":{}, "axis_coordinates":{},
                "array":{"dtype":"float64", "shape":[rows,7], "values":x}}],
            "y":if labelled {json!({"dtype":"float64","shape":[rows,2],"values":y})}
                else {Value::Null},
            "target_names":if labelled {json!(["protein","moisture"])} else {json!([])},
            "groups":null,
            "partitions":{"dtype":"<U7","shape":[rows],
                "values":vec![if training {"train"} else {"predict"}; rows]}
        }
    });
    if training {
        result["fold_ids"] = json!((0..rows)
            .map(|i| format!("fold:{}", i % 3))
            .collect::<Vec<_>>());
    }
    result
}

pub fn recipe_value(selected: bool) -> Value {
    let mut recipe = json!({"steps":[
        {"method_id":"preprocessing.scaling.standard_scale","role":"transformer","params":{}},
        {"method_id":"models.regularized.ridge","role":"regressor","params":{"scale_x":false}}
    ],"candidates":[{"alpha":0.1},{"alpha":1.0}]});
    if selected {
        recipe["steps"][1]["params"]["native_target_index"] = json!(1);
    }
    recipe
}

pub fn expected_names(selected: bool) -> Vec<String> {
    if selected {
        vec!["moisture".into()]
    } else {
        vec!["protein".into(), "moisture".into()]
    }
}

/// Exact synthetic installed-cohort record: permutation (7p) mod 40.
pub fn interleaved_record() -> Value {
    serde_json::from_str(r#"{"schema":"nirs4all.dataset.v1","schema_version":1,"dataset":{"schema":"nirs4all.multimodal-dataset","schema_version":1,"name":"cluster-local-synthetic","sample_ids":["sample-000","sample-007","sample-014","sample-021","sample-028","sample-035","sample-002","sample-009","sample-016","sample-023","sample-030","sample-037","sample-004","sample-011","sample-018","sample-025","sample-032","sample-039","sample-006","sample-013","sample-020","sample-027","sample-034","sample-001","sample-008","sample-015","sample-022","sample-029","sample-036","sample-003","sample-010","sample-017","sample-024","sample-031","sample-038","sample-005","sample-012","sample-019","sample-026","sample-033"],"source_alignment":"strict","sources":[{"name":"nir","sample_ids":["sample-000","sample-007","sample-014","sample-021","sample-028","sample-035","sample-002","sample-009","sample-016","sample-023","sample-030","sample-037","sample-004","sample-011","sample-018","sample-025","sample-032","sample-039","sample-006","sample-013","sample-020","sample-027","sample-034","sample-001","sample-008","sample-015","sample-022","sample-029","sample-036","sample-003","sample-010","sample-017","sample-024","sample-031","sample-038","sample-005","sample-012","sample-019","sample-026","sample-033"],"representation_id":"signal_1d","axes":["sample","wavelength"],"feature_names":null,"axis_units":{"wavelength":"nm"},"axis_coordinates":{"wavelength":[1100.0,1200.0,1300.0]},"array":{"dtype":"float64","shape":[40,3],"values":[[1.0,2.0,5.0],[8.0,2.0,3.0577776593313417],[15.0,2.0,4.77556587851025],[22.0,2.0,3.4807113458833143],[29.0,2.0,4.2030048638187525],[36.0,2.0,4.136737218207833],[3.0,4.0,4.696706709347166],[10.0,4.0,3.103241583665853],[17.0,4.0,4.9931849187581925],[24.0,4.0,3.025156378595836],[31.0,4.0,4.843853958732492],[38.0,4.0,3.384647517045279],[5.0,6.0,3.9708004776987114],[12.0,6.0,3.6926671300215808],[19.0,6.0,4.608351314532254],[26.0,6.0,3.1609284709235475],[33.0,6.0,4.972832565697435],[40.0,6.0,3.0058223748161845],[7.0,8.0,3.2626062844587542],[14.0,8.0,4.468516671300377],[21.0,8.0,3.8544999661913866],[28.0,8.0,3.805670093544665],[35.0,8.0,4.511703992453148],[2.0,3.0,4.921060994002885],[9.0,3.0,3.0017052242052467],[16.0,3.0,4.960170286650366],[23.0,3.0,3.188906985938344],[30.0,3.0,4.568289629767975],[37.0,3.0,3.740182643786244],[4.0,5.0,4.362357754476673],[11.0,5.0,3.346356379136388],[18.0,5.0,4.869397490349825],[25.0,5.0,3.0153121442058732],[32.0,5.0,4.986192302278864],[39.0,5.0,3.126263016988919],[6.0,7.0,3.5838531634528574],[13.0,7.0,4.087498983439447],[20.0,7.0,4.251259842582255],[27.0,7.0,3.439015742572771],[34.0,7.0,4.80588395764045]]},"presence_mask":{"dtype":"bool","shape":[40],"values":[true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true,true]}}],"y":{"dtype":"float64","shape":[40,1],"values":[[2.0],[16.59700526185473],[30.290101325857954],[42.824566496763076],[57.20933433489143],[72.48433404236036],[5.328759684178084],[19.594495947611364],[32.94026284032406],[45.611340272085705],[60.68935529354758],[75.57563759922068],[8.511516186439264],[22.547029829823906],[35.4791498196116],[48.67326141952157],[64.11111571798999],[78.59970782033784],[11.584560342858977],[25.40920470608154],[38.014097497930344],[51.98836953115294],[67.39645414962924],[3.6828815295956385],[18.099914647882553],[31.63315406538994],[44.18617932329815],[58.949055746539834],[74.0417043926064],[6.93711402429037],[21.07856870265296],[34.21833839604313],[47.10689782000534],[62.41414905763828],[77.09317227542927],[10.058241617498469],[23.992962977239042],[36.73839407757373],[50.304348492794404],[65.77271815504147]]},"target_names":["response"],"target_mask":{"dtype":"bool","shape":[40,1],"values":[[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true],[true]]},"task_type":"regression","groups":null,"partitions":{"dtype":"<U5","shape":[40],"values":["train","train","train","train","train","test","train","train","train","train","train","test","train","train","train","train","test","test","train","train","train","train","test","train","train","train","train","train","test","train","train","train","train","train","test","train","train","train","train","test"]}},"origin_ids":["sample-000","sample-007","sample-014","sample-021","sample-028","sample-035","sample-002","sample-009","sample-016","sample-023","sample-030","sample-037","sample-004","sample-011","sample-018","sample-025","sample-032","sample-039","sample-006","sample-013","sample-020","sample-027","sample-034","sample-001","sample-008","sample-015","sample-022","sample-029","sample-036","sample-003","sample-010","sample-017","sample-024","sample-031","sample-038","sample-005","sample-012","sample-019","sample-026","sample-033"],"fold_ids":["fold-0","fold-3","fold-2","fold-1","fold-0",null,"fold-2","fold-1","fold-0","fold-3","fold-2",null,"fold-0","fold-3","fold-2","fold-1",null,null,"fold-2","fold-1","fold-0","fold-3",null,"fold-1","fold-0","fold-3","fold-2","fold-1",null,"fold-3","fold-2","fold-1","fold-0","fold-3",null,"fold-1","fold-0","fold-3","fold-2",null]}"#).unwrap()
}

/// The same training rows, in IO audit order, with the same declared folds.
pub fn training_cohort(record: &Value) -> Value {
    let rows: Vec<_> = record["dataset"]["partitions"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, partition)| *partition == "train")
        .map(|(row, _)| row)
        .collect();
    let pick = |values: &Value| -> Value {
        json!(rows
            .iter()
            .map(|row| values[*row].clone())
            .collect::<Vec<_>>())
    };
    let mut out = record.clone();
    for field in ["origin_ids", "fold_ids"] {
        out[field] = pick(&record[field]);
    }
    out["dataset"]["sample_ids"] = pick(&record["dataset"]["sample_ids"]);
    for field in ["y", "target_mask", "partitions"] {
        out["dataset"][field]["values"] = pick(&record["dataset"][field]["values"]);
        out["dataset"][field]["shape"][0] = json!(rows.len());
    }
    let source = &record["dataset"]["sources"][0];
    out["dataset"]["sources"][0]["sample_ids"] = pick(&source["sample_ids"]);
    for field in ["array", "presence_mask"] {
        out["dataset"]["sources"][0][field]["values"] = pick(&source[field]["values"]);
        out["dataset"]["sources"][0][field]["shape"][0] = json!(rows.len());
    }
    out
}
