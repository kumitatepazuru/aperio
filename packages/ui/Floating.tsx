import React, {
  createContext,
  forwardRef,
  useCallback,
  useContext,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  arrow as arrowMiddleware,
  autoPlacement,
  autoUpdate,
  FloatingPortal,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useHover,
  useInteractions,
} from "@floating-ui/react";

// ─── Context ──────────────────────────────────────────────────────────────────

type GetProps = (
  userProps?: React.HTMLProps<Element>,
) => Record<string, unknown>;

interface FloatingContextValue {
  isOpen: boolean;
  setReference: React.RefCallback<Element>;
  setFloating: React.RefCallback<HTMLElement>;
  floatingStyles: React.CSSProperties;
  getReferenceProps: GetProps;
  getFloatingProps: GetProps;
  arrowRef: React.RefObject<HTMLDivElement | null>;
  middlewareData: { arrow?: { x?: number; y?: number } };
  placement: string;
}

const FloatingCtx = createContext<FloatingContextValue | null>(null);

function useFloatingCtx(): FloatingContextValue {
  const ctx = useContext(FloatingCtx);
  if (!ctx) throw new Error("Must be used inside <FloatingBase>");
  return ctx;
}

// ─── FloatingBase ─────────────────────────────────────────────────────────────

export interface FloatingHandle {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  switch: () => void;
}

interface FloatingBaseProps {
  children: React.ReactNode;
  onChange?: (isOpen: boolean) => void;
}

export const FloatingBase = forwardRef<FloatingHandle, FloatingBaseProps>(
  function FloatingBase({ children, onChange }, ref) {
    // Read click/hover flags from <Reference> children before hooks
    let clickEnabled = false;
    let hoverEnabled = false;
    React.Children.forEach(children, (child) => {
      if (
        React.isValidElement(child) &&
        (child.type as { __isFloatingReference?: boolean })
          .__isFloatingReference
      ) {
        const p = child.props as ReferenceProps;
        clickEnabled = p.click ?? false;
        hoverEnabled = p.hover ?? false;
      }
    });

    const arrowRef = useRef<HTMLDivElement>(null);
    const [isOpen, setIsOpen] = useState(false);

    const handleOpenChange = useCallback(
      (open: boolean) => {
        setIsOpen(open);
        onChange?.(open);
      },
      [onChange],
    );

    const { refs, floatingStyles, context, middlewareData, placement } =
      useFloating({
        open: isOpen,
        onOpenChange: handleOpenChange,
        middleware: [
          autoPlacement(),
          shift(),
          arrowMiddleware({ element: arrowRef }),
        ],
        whileElementsMounted: autoUpdate,
      });

    const clickInteraction = useClick(context, { enabled: clickEnabled });
    const hoverInteraction = useHover(context, { enabled: hoverEnabled });
    const dismiss = useDismiss(context);

    const { getReferenceProps, getFloatingProps } = useInteractions([
      clickInteraction,
      hoverInteraction,
      dismiss,
    ]);

    useImperativeHandle(
      ref,
      () => ({
        get isOpen() {
          return isOpen;
        },
        open: () => handleOpenChange(true),
        close: () => handleOpenChange(false),
        switch: () => handleOpenChange(!isOpen),
      }),
      [isOpen, handleOpenChange],
    );

    const ctxValue = useMemo<FloatingContextValue>(
      () => ({
        isOpen,
        setReference: refs.setReference,
        setFloating: refs.setFloating,
        floatingStyles,
        getReferenceProps,
        getFloatingProps: getFloatingProps as GetProps,
        arrowRef,
        middlewareData,
        placement,
      }),
      [
        isOpen,
        refs,
        floatingStyles,
        getReferenceProps,
        getFloatingProps,
        middlewareData,
        placement,
      ],
    );

    return (
      <FloatingCtx.Provider value={ctxValue}>{children}</FloatingCtx.Provider>
    );
  },
);

// ─── Reference ────────────────────────────────────────────────────────────────

interface ReferenceProps {
  children: React.ReactNode;
  /** Enable click to open/close */
  click?: boolean;
  /** Enable hover to open/close */
  hover?: boolean;
  className?: string;
  style?: React.CSSProperties;
}

export function Reference({ children, className, style }: ReferenceProps) {
  const { setReference, getReferenceProps } = useFloatingCtx();
  return (
    <div
      ref={setReference}
      {...getReferenceProps({
        className,
        style,
      })}
    >
      {children}
    </div>
  );
}

// Static flag so FloatingBase can detect this component type in children
(Reference as { __isFloatingReference?: boolean }).__isFloatingReference = true;

// ─── Floating ─────────────────────────────────────────────────────────────────

const OPPOSITE_SIDE = {
  top: "bottom",
  right: "left",
  bottom: "top",
  left: "right",
};

interface FloatingProps {
  children: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
}

export function Floating({ children, className, style }: FloatingProps) {
  const {
    isOpen,
    setFloating,
    floatingStyles,
    getFloatingProps,
    arrowRef,
    middlewareData,
    placement,
  } = useFloatingCtx();

  if (!isOpen) return null;

  const baseSide = placement.split("-")[0] as keyof typeof OPPOSITE_SIDE;
  const staticSide = OPPOSITE_SIDE[baseSide] ?? "bottom";

  return (
    <FloatingPortal>
      <div
        ref={setFloating}
        style={{ ...floatingStyles }}
        {...getFloatingProps()}
      >
        <div
          className={`z-50 shadow-lg rounded-xl bg-base-200 ${className}`}
          style={style}
        >
          {children}
        </div>
        <div
          ref={arrowRef}
          className="absolute w-2 h-2 bg-base-200 rotate-45"
          style={{
            left: middlewareData.arrow?.x,
            top: middlewareData.arrow?.y,
            [staticSide]: "-4px",
          }}
        />
      </div>
    </FloatingPortal>
  );
}
