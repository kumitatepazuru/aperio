import React, { useEffect, useRef, useState } from "react";
import {
  autoUpdate,
  flip,
  FloatingNode,
  FloatingPortal,
  FloatingTree,
  offset,
  safePolygon,
  shift,
  useClick,
  useDismiss,
  useFloating,
  useFloatingNodeId,
  useFloatingTree,
  useHover,
  useInteractions,
  useListNavigation,
  useMergeRefs,
} from "@floating-ui/react";
import { MdArrowRight } from "react-icons/md";

// ─── Types ────────────────────────────────────────────────────────────────────
type Awaitable<T> = T | Promise<T>;

export type MenuItem = {
  id: string;
  type: "item";
  value: string;
  click?: (value: string) => Awaitable<boolean>;
};

export type SubMenuDef = {
  id: string;
  type: "submenu";
  value: string;
  submenu: (MenuItem | SubMenuDef)[];
};

export type NestedMenuItems = (MenuItem | SubMenuDef)[];

// ─── ItemList (shared renderer for one panel's items) ─────────────────────────

interface ItemListProps {
  items: NestedMenuItems;
  listRef: React.RefObject<Array<HTMLElement | null>>;
  activeIndex: number | null;
  getItemProps: (
    props?: React.HTMLProps<HTMLElement>,
  ) => Record<string, unknown>;
}

const ItemList = ({
  items,
  listRef,
  activeIndex,
  getItemProps,
}: ItemListProps) => (
  <>
    {items.map((item, index) => {
      if (item.type === "item") {
        return (
          <LeafItem
            key={item.id}
            item={item}
            ref={(el) => {
              listRef.current[index] = el;
            }}
            tabIndex={activeIndex === index ? 0 : -1}
            {...getItemProps()}
          />
        );
      } else {
        return (
          <SubMenuNode
            key={item.id}
            item={item}
            ref={(el) => {
              listRef.current[index] = el;
            }}
            {...getItemProps()}
          />
        );
      }
    })}
  </>
);

// ─── LeafItem ─────────────────────────────────────────────────────────────────

interface LeafItemProps extends React.HTMLAttributes<HTMLButtonElement> {
  item: MenuItem;
  ref?: React.Ref<HTMLButtonElement>;
}

const LeafItem = ({ item, ref, ...props }: LeafItemProps) => {
  const tree = useFloatingTree();
  const handleClick = async () => {
    const result = await item.click?.(item.id);
    if (result) tree?.events.emit("close-all");
  };
  return (
    <button
      ref={ref}
      {...props}
      onClick={handleClick}
      className="btn btn-sm join-item"
    >
      {item.value}
    </button>
  );
};

// ─── SubMenuNode (recursive) ──────────────────────────────────────────────────

interface SubMenuNodeProps extends React.HTMLAttributes<HTMLButtonElement> {
  item: SubMenuDef;
  ref?: React.Ref<HTMLButtonElement>;
}

const SubMenuNode = ({
  item,
  ref: forwardedRef,
  ...itemProps
}: SubMenuNodeProps) => {
  const [isOpen, setIsOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const nodeId = useFloatingNodeId();
  const tree = useFloatingTree();
  const listRef = useRef<Array<HTMLElement | null>>([]);

  const { refs, floatingStyles, context } = useFloating({
    nodeId,
    open: isOpen,
    onOpenChange: setIsOpen,
    placement: "right-start",
    middleware: [offset({ mainAxis: 4, alignmentAxis: -4 }), flip(), shift()],
    whileElementsMounted: autoUpdate,
  });

  const hoverInteraction = useHover(context, {
    delay: { open: 75 },
    handleClose: safePolygon({ blockPointerEvents: true }),
  });
  const dismiss = useDismiss(context, { bubbles: true });
  const listNav = useListNavigation(context, {
    listRef,
    activeIndex,
    onNavigate: setActiveIndex,
    nested: true,
  });

  const { getReferenceProps, getFloatingProps, getItemProps } = useInteractions(
    [hoverInteraction, dismiss, listNav],
  );

  useEffect(() => {
    if (!tree) return;
    const close = () => setIsOpen(false);
    tree.events.on("close-all", close);
    return () => tree.events.off("close-all", close);
  }, [tree]);

  return (
    <FloatingNode id={nodeId}>
      <button
        ref={useMergeRefs([
          refs.setReference as React.Ref<HTMLButtonElement>,
          forwardedRef ?? null,
        ])}
        {...getReferenceProps(itemProps as React.HTMLProps<Element>)}
        className="btn btn-sm join-item gap-3"
      >
        <span className="grow">{item.value}</span>
        <MdArrowRight size="1.25em" />
      </button>

      {isOpen && (
        <FloatingPortal>
          <div
            ref={refs.setFloating}
            style={floatingStyles}
            {...getFloatingProps({
              className: "z-50 shadow-lg rounded-xl join join-vertical",
            })}
          >
            <ItemList
              items={item.submenu}
              listRef={listRef}
              activeIndex={activeIndex}
              getItemProps={getItemProps as ItemListProps["getItemProps"]}
            />
          </div>
        </FloatingPortal>
      )}
    </FloatingNode>
  );
};

// ─── NestedMenu (public API) ──────────────────────────────────────────────────

interface NestedMenuProps {
  items: NestedMenuItems | (() => Promise<NestedMenuItems>);
  children: React.ReactNode;
  click?: boolean;
  hover?: boolean;
}

const NestedMenuInner = ({
  items,
  click,
  hover,
  children,
}: NestedMenuProps) => {
  const [isOpen, setIsOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [latestItems, setLatestItems] = useState(
    items instanceof Function ? [] : items,
  );
  const nodeId = useFloatingNodeId();
  const tree = useFloatingTree();
  const listRef = useRef<Array<HTMLElement | null>>([]);

  const { refs, floatingStyles, context } = useFloating({
    nodeId,
    open: isOpen,
    onOpenChange: (open, _event, reason) => {
      if (open && reason === "hover" && !hover) return;
      if (typeof items === "function") {
        items().then((resolvedItems) => {
          setLatestItems(resolvedItems);
          setIsOpen(open);
        });
      } else {
        setIsOpen(open);
      }
    },
    placement: "bottom-start",
    middleware: [offset(4), flip(), shift()],
    whileElementsMounted: autoUpdate,
  });

  const clickInteraction = useClick(context, { enabled: click ?? false });
  const hoverInteraction = useHover(context, {
    delay: { open: 75 },
    handleClose: safePolygon({ blockPointerEvents: true }),
  });
  const dismiss = useDismiss(context, { bubbles: true });
  const listNav = useListNavigation(context, {
    listRef,
    activeIndex,
    onNavigate: setActiveIndex,
  });

  const { getReferenceProps, getFloatingProps, getItemProps } = useInteractions(
    [clickInteraction, hoverInteraction, dismiss, listNav],
  );

  useEffect(() => {
    if (!tree) return;
    const close = () => setIsOpen(false);
    tree.events.on("close-all", close);
    return () => tree.events.off("close-all", close);
  }, [tree]);

  return (
    <FloatingNode id={nodeId}>
      <div ref={refs.setReference} {...getReferenceProps()}>
        {children}
      </div>

      {isOpen && (
        <FloatingPortal>
          <div
            ref={refs.setFloating}
            style={floatingStyles}
            {...getFloatingProps({
              className: "z-50 shadow-lg rounded-xl join join-vertical",
            })}
          >
            <ItemList
              items={latestItems}
              listRef={listRef}
              activeIndex={activeIndex}
              getItemProps={getItemProps as ItemListProps["getItemProps"]}
            />
          </div>
        </FloatingPortal>
      )}
    </FloatingNode>
  );
};

const NestedMenu = (props: NestedMenuProps) => (
  <FloatingTree>
    <NestedMenuInner {...props} />
  </FloatingTree>
);

export default NestedMenu;
