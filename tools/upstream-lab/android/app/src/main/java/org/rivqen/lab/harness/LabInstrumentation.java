// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;

import java.io.File;
import java.util.regex.Pattern;

/**
 * Entry point: one run per {@code am instrument} (CONTRACT section 4).
 *
 * <pre>
 * am instrument -w -e run &lt;run-id&gt; -e control http://10.0.2.2:18091 \
 *     org.rivqen.lab.harness/org.rivqen.lab.harness.LabInstrumentation
 * am instrument -w -e mode functions org.rivqen.lab.harness/org.rivqen.lab.harness.LabInstrumentation
 * </pre>
 *
 * The scenario run gets everything else from the control channel. The function run reads
 * {@code files/functions.json} and writes {@code files/functions.jsonl} in the app storage.
 */
public final class LabInstrumentation extends Instrumentation {

    private static final Pattern RUN_ID = Pattern.compile("^[A-Za-z0-9._:/-]{1,128}$");

    private Bundle arguments = new Bundle();

    @Override
    public void onCreate(Bundle args) {
        super.onCreate(args);
        if (args != null) {
            arguments = args;
        }
        start();
    }

    @Override
    public void onStart() {
        super.onStart();
        Bundle result = new Bundle();
        result.putString("rivqen.upstream_sha", BuildConfig.UPSTREAM_SHA);
        String error;
        try {
            if ("functions".equals(arguments.getString("mode", "scenario"))) {
                error = functions(result);
            } else {
                error = scenario(result);
            }
        } catch (Throwable e) {
            error = Lab.describe(e);
        }
        result.putBoolean("rivqen.ok", error == null);
        if (error != null) {
            result.putString("rivqen.error", error);
            Lab.log("run failed: " + error);
        }
        finish(error == null ? Activity.RESULT_OK : Activity.RESULT_CANCELED, result);
    }

    private String scenario(Bundle result) throws Exception {
        String run = arguments.getString("run");
        if (run == null || !RUN_ID.matcher(run).matches()) {
            return "argument run is missing or not [A-Za-z0-9._:/-]{1,128}";
        }
        result.putString("rivqen.run", run);
        ControlChannel control = new ControlChannel(arguments.getString("control"));
        ControlEvents events = new ControlEvents(control);
        ScenarioRun scenarioRun = new ScenarioRun(getTargetContext(), control, events, run, () -> {
            Intent intent = new Intent(getTargetContext(), WebViewActivity.class);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            WebViewActivity activity = (WebViewActivity) startActivitySync(intent);
            activity.attach(events);
            waitForIdleSync();
            return activity;
        });
        String error;
        try {
            error = scenarioRun.execute();
        } catch (Exception e) {
            error = Lab.describe(e);
        } finally {
            scenarioRun.close();
        }
        if (error != null) {
            // A harness failure ends the run; a normal end is reported by the conductor.
            events.emit("run.end", "ok", false, "error", error);
        }
        events.close(30_000);
        return error;
    }

    private String functions(Bundle result) throws Exception {
        File dir = getTargetContext().getFilesDir();
        FunctionCases cases = new FunctionCases(getTargetContext());
        int[] counts = cases.run(new File(dir, "functions.json"), new File(dir, "functions.jsonl"));
        result.putInt("rivqen.cases", counts[0]);
        result.putInt("rivqen.unsupported", counts[1]);
        result.putInt("rivqen.errors", counts[2]);
        return null;
    }
}
