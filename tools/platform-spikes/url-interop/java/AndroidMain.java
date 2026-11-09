// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import android.net.Uri;
import android.os.Build;

/** Android runner (app_process, no APK): android.net.Uri.parse(input).toString(). */
public final class AndroidMain {
    public static void main(String[] args) throws Exception {
        String runtime = "Android " + Build.VERSION.RELEASE + " (API " + Build.VERSION.SDK_INT + "), " + Build.MODEL + ", " + Build.FINGERPRINT;
        Common.run(args[0], "android-net-uri", "android.net.Uri of the framework (API " + Build.VERSION.SDK_INT + "); API: Uri.parse(input), toString(), getHost(), getEncodedPath(), getEncodedQuery()", runtime,
            (input, base) -> {
                if (base != null) {
                    // android.net.Uri has no resolve method.
                    throw new UnsupportedOperationException("android.net.Uri has no resolve API");
                }
                Uri u = Uri.parse(input);
                return new String[] {u.toString(), u.getHost(), u.getEncodedPath(), u.getEncodedQuery()};
            });
    }
}
