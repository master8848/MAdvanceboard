package com.kb.ime;

import android.view.View;
import androidx.lifecycle.Lifecycle;
import androidx.lifecycle.LifecycleOwner;
import androidx.lifecycle.LifecycleRegistry;
import androidx.lifecycle.ViewModelStore;
import androidx.lifecycle.ViewModelStoreOwner;
import androidx.lifecycle.ViewTreeLifecycleOwner;
import androidx.lifecycle.ViewTreeViewModelStoreOwner;
import androidx.savedstate.SavedStateRegistry;
import androidx.savedstate.SavedStateRegistryController;
import androidx.savedstate.SavedStateRegistryOwner;
import androidx.savedstate.ViewTreeSavedStateRegistryOwner;

/**
 * Window owners for the IME's {@code ComposeView} strip, written in Java on
 * purpose: kotlinc 2.3.20 in this build cannot resolve
 * {@code androidx.lifecycle.ViewTreeLifecycleOwner} (verified present in
 * {@code lifecycle-runtime-release-api.jar} on the compile classpath and
 * consumable by javac — same root cause undiagnosed), while javac consumes
 * it fine. If a future Kotlin/AGP bump fixes the resolution, this bridge
 * can be inlined back into {@code KbInputMethodService}.
 *
 * <p>{@code InputMethodService} windows provide no view-tree owners; without
 * them the strip crashes on attach — first {@code ViewTreeLifecycleOwner
 * not found} (the keyboard could never open), then
 * {@code ViewTreeSavedStateRegistryOwner} (both caught on-device). This
 * class supplies all three owners Compose requires (lifecycle +
 * saved-state registry + view-model store) with service-driven transitions.
 *
 * <p>Owner tags are applied to the whole ancestor chain on attach, not just
 * the input root: the IME window (a Dialog) wraps our view in framework
 * chrome, and Compose climbs to the window content child before looking the
 * owners up — a tag on our root alone is invisible to it. The null-guards
 * never overwrite owners a host already provides.
 */
public final class ImeLifecycle
        implements LifecycleOwner, SavedStateRegistryOwner, ViewModelStoreOwner {
    private final LifecycleRegistry lifecycleRegistry = new LifecycleRegistry(this);
    private final SavedStateRegistryController savedStateController =
            SavedStateRegistryController.create(this);
    private final ViewModelStore viewModelStore = new ViewModelStore();
    private boolean savedStateAttached = false;

    @Override
    public Lifecycle getLifecycle() {
        return lifecycleRegistry;
    }

    @Override
    public SavedStateRegistry getSavedStateRegistry() {
        return savedStateController.getSavedStateRegistry();
    }

    @Override
    public ViewModelStore getViewModelStore() {
        return viewModelStore;
    }

    public void handle(Lifecycle.Event event) {
        if (event == Lifecycle.Event.ON_CREATE && !savedStateAttached) {
            savedStateAttached = true;
            savedStateController.performAttach();
            savedStateController.performRestore(null);
        }
        if (event == Lifecycle.Event.ON_DESTROY) {
            viewModelStore.clear();
        }
        lifecycleRegistry.handleLifecycleEvent(event);
    }

    public static void attachTo(View root, ImeLifecycle owner) {
        setOnChain(root, owner);
        root.addOnAttachStateChangeListener(new View.OnAttachStateChangeListener() {
            @Override
            public void onViewAttachedToWindow(View v) {
                setOnChain(v, owner);
            }

            @Override
            public void onViewDetachedFromWindow(View v) {}
        });
        android.util.Log.i("KbIME", "ImeLifecycle.attachTo root=" + root);
    }

    private static void setOnChain(View leaf, ImeLifecycle owner) {
        View current = leaf;
        while (current != null) {
            if (ViewTreeLifecycleOwner.get(current) == null) {
                ViewTreeLifecycleOwner.set(current, owner);
            }
            if (ViewTreeSavedStateRegistryOwner.get(current) == null) {
                ViewTreeSavedStateRegistryOwner.set(current, owner);
            }
            if (ViewTreeViewModelStoreOwner.get(current) == null) {
                ViewTreeViewModelStoreOwner.set(current, owner);
            }
            android.view.ViewParent parent = current.getParent();
            current = (parent instanceof View) ? (View) parent : null;
        }
    }
}
