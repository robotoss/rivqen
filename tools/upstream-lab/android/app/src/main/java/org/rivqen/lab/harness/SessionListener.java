// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
package org.rivqen.lab.harness;

/**
 * Receives the session callbacks of the SDK through {@code com.tencent.sonic.sdk.RivqenLabAccess}.
 * {@code args} holds codes only (CONTRACT section 5: codes or URL only); page text is never passed.
 */
public interface SessionListener {

    void onCallback(String name, int[] args);
}
